use super::api::{self, nodes_server};
use super::helpers;
use crate::auth::FindableById;
use crate::{auth, models};
use diesel_async::scoped_futures::ScopedFutureExt;
use futures_util::future::OptionFuture;
use std::collections::HashMap;
use tonic::{Request, Status};

#[tonic::async_trait]
impl nodes_server::Nodes for super::GrpcImpl {
    async fn create(
        &self,
        request: Request<api::CreateNodeRequest>,
    ) -> super::Result<api::CreateNodeResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, auth::UserAuthToken>(&request)?.clone();
        // Check quota
        let mut conn = self.conn().await?;
        let user = models::User::find_by_id(token.id, &mut conn).await?;

        if user.staking_quota <= 0 {
            return Err(Status::resource_exhausted("User node quota exceeded"));
        }

        let inner = request.into_inner();
        self.trx(|c| {
            async move {
                let new_node = inner.as_new(user.id)?;
                let node = new_node.create(c).await?;

                let new_command = models::NewCommand {
                    host_id: node.host_id,
                    cmd: models::HostCmd::CreateNode,
                    sub_cmd: None,
                    node_id: Some(node.id),
                };
                let create_cmd = new_command.create(c).await?;

                let update_user = models::UpdateUser {
                    id: user.id,
                    first_name: None,
                    last_name: None,
                    fee_bps: None,
                    staking_quota: Some(user.staking_quota - 1),
                    refresh: None,
                };
                update_user.update(c).await?;

                let new_command = models::NewCommand {
                    host_id: node.host_id,
                    cmd: models::HostCmd::RestartNode,
                    sub_cmd: None,
                    node_id: Some(node.id),
                };
                let restart_cmd = new_command.create(c).await?;

                let created = api::NodeMessage::created(node.clone(), user.clone(), c).await?;
                let create_msg = api::Command::from_model(&create_cmd, c).await?;
                let restart_msg = api::Command::from_model(&restart_cmd, c).await?;
                self.notifier.nodes_sender().send(&created).await?;
                self.notifier.commands_sender().send(&create_msg).await?;
                self.notifier.commands_sender().send(&restart_msg).await?;

                let response = api::CreateNodeResponse {
                    node: Some(api::Node::from_model(node.clone(), c).await?),
                };

                Ok(super::response_with_refresh_token(refresh_token, response)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn get(
        &self,
        request: Request<api::GetNodeRequest>,
    ) -> super::Result<api::GetNodeResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let org_id = helpers::try_get_token::<_, auth::UserAuthToken>(&request)
            .ok()
            .map(|t| t.try_org_id())
            .transpose()?;
        let host_id = helpers::try_get_token::<_, auth::HostAuthToken>(&request)
            .ok()
            .map(|t| t.id);
        let inner = request.into_inner();
        let node_id = inner.id.parse().map_err(crate::Error::from)?;
        let mut conn = self.conn().await?;
        let node = models::Node::find_by_id(node_id, &mut conn).await?;

        let is_allowed = if let Some(org_id) = org_id {
            node.org_id == org_id
        } else if let Some(host_id) = host_id {
            node.host_id == host_id
        } else {
            false
        };

        if !is_allowed {
            super::bail_unauthorized!("Access not allowed")
        }
        let response = api::GetNodeResponse {
            node: Some(api::Node::from_model(node, &mut conn).await?),
        };
        super::response_with_refresh_token(refresh_token, response)
    }

    async fn list(
        &self,
        request: Request<api::ListNodesRequest>,
    ) -> super::Result<api::ListNodesResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let request = request.into_inner();
        let mut conn = self.conn().await?;
        let nodes = models::Node::filter(request.as_filter()?, &mut conn).await?;
        let nodes = api::Node::from_models(nodes, &mut conn).await?;
        let response = api::ListNodesResponse { nodes };
        super::response_with_refresh_token(refresh_token, response)
    }

    async fn update(
        &self,
        request: Request<api::UpdateNodeRequest>,
    ) -> super::Result<api::UpdateNodeResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let user_token = helpers::try_get_token::<_, auth::UserAuthToken>(&request).ok();
        let org_id = user_token.map(|t| t.try_org_id()).transpose()?;
        let user_id = user_token.map(|t| t.id);
        let host_id = helpers::try_get_token::<_, auth::HostAuthToken>(&request)
            .ok()
            .map(|t| t.id);

        self.trx(|c| {
            async move {
                let inner = request.into_inner();
                let node = models::Node::find_by_id(inner.id.parse()?, c).await?;

                let is_allowed = if let Some(org_id) = org_id {
                    node.org_id == org_id
                } else if let Some(host_id) = host_id {
                    node.host_id == host_id
                } else {
                    false
                };

                if !is_allowed {
                    super::bail_unauthorized!("Access not allowed")
                }

                let update_node = inner.as_update()?;
                let user = user_id.map(|id| models::User::find_by_id(id, c));
                let user = OptionFuture::from(user).await.transpose()?;
                let node = update_node.update(c).await?;

                let msg = api::NodeMessage::updated(node, user, c).await?;
                self.notifier.nodes_sender().send(&msg).await?;

                let response = api::UpdateNodeResponse {};
                Ok(super::response_with_refresh_token(refresh_token, response)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn delete(
        &self,
        request: Request<api::DeleteNodeRequest>,
    ) -> super::Result<api::DeleteNodeResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let user_id = helpers::try_get_token::<_, auth::UserAuthToken>(&request)?.id;
        let inner = request.into_inner();
        self.trx(|c| {
            async move {
                let node_id = inner.id.parse()?;
                let node = models::Node::find_by_id(node_id, c).await?;

                if !models::Node::belongs_to_user_org(node.org_id, user_id, c).await? {
                    super::bail_unauthorized!("User cannot delete node");
                }
                // 1. Delete node, if the node belongs to the current user
                // Key files are deleted automatically because of 'on delete cascade' in tables DDL
                models::Node::delete(node_id, c).await?;

                let host_id = node.host_id;
                // 2. Do NOT delete reserved IP addresses, but set assigned to false
                let ip_addr = node
                    .ip_addr
                    .parse()
                    .map_err(|_| Status::internal("invalid ip"))?;
                let ip = models::IpAddress::find_by_node(ip_addr, c).await?;

                models::IpAddress::unassign(ip.id, host_id, c).await?;

                // Delete all pending commands for this node: there are not useable anymore
                models::Command::delete_pending(node_id, c).await?;

                // Send delete node command
                let node_id = node_id.to_string();
                let new_command = models::NewCommand {
                    host_id: node.host_id,
                    cmd: models::HostCmd::DeleteNode,
                    sub_cmd: Some(&node_id),
                    // Note that the `node_id` goes into the `sub_cmd` field, not the node_id
                    // field, because the node was just deleted.
                    node_id: None,
                };
                let cmd = new_command.create(c).await?;

                let user = models::User::find_by_id(user_id, c).await?;
                let update_user = models::UpdateUser {
                    id: user.id,
                    first_name: None,
                    last_name: None,
                    fee_bps: None,
                    staking_quota: Some(user.staking_quota + 1),
                    refresh: None,
                };
                update_user.update(c).await?;

                let cmd = api::Command::from_model(&cmd, c).await?;
                self.notifier.commands_sender().send(&cmd).await?;

                let deleted = api::NodeMessage::deleted(node, user);
                self.notifier.nodes_sender().send(&deleted).await?;
                Ok(())
            }
            .scope_boxed()
        })
        .await?;
        let resp = api::DeleteNodeResponse {};
        super::response_with_refresh_token(refresh_token, resp)
    }
}

impl api::Node {
    /// This function is used to create a ui node from a database node. We want to include the
    /// `database_name` in the ui representation, but it is not in the node model. Therefore we
    /// perform a seperate query to the blockchains table.
    pub async fn from_model(
        node: models::Node,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> crate::Result<Self> {
        let blockchain = models::Blockchain::find_by_id(node.blockchain_id, conn).await?;
        let user_fut = node
            .created_by
            .map(|u_id| models::User::find_by_id(u_id, conn));
        let user = OptionFuture::from(user_fut).await.transpose()?;
        Self::new(node, &blockchain, user.as_ref())
    }

    /// This function is used to create many ui nodes from many database nodes. The same
    /// justification as above applies. Note that this function does not simply defer to the
    /// function above, but rather it performs 1 query for n nodes. We like it this way :)
    pub async fn from_models(
        nodes: Vec<models::Node>,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let blockchain_ids: Vec<_> = nodes.iter().map(|n| n.blockchain_id).collect();
        let blockchains: HashMap<_, _> = models::Blockchain::find_by_ids(&blockchain_ids, conn)
            .await?
            .into_iter()
            .map(|b| (b.id, b))
            .collect();
        let user_ids: Vec<_> = nodes.iter().flat_map(|n| n.created_by).collect();
        let users: HashMap<_, _> = models::User::find_by_ids(&user_ids, conn)
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();

        nodes
            .into_iter()
            .map(|n| (n.blockchain_id, n.created_by, n))
            .map(|(b_id, u_id, n)| {
                Self::new(
                    n,
                    &blockchains[&b_id],
                    u_id.and_then(|u_id| users.get(&u_id)),
                )
            })
            .collect()
    }

    /// Construct a new ui node from the queried parts.
    fn new(
        node: models::Node,
        blockchain: &models::Blockchain,
        user: Option<&models::User>,
    ) -> crate::Result<Self> {
        use api::node::{ContainerStatus, NodeStatus, NodeType, StakingStatus, SyncStatus};

        let properties = node
            .properties()?
            .properties
            .into_iter()
            .flatten()
            .map(api::node::NodeProperty::from_model)
            .collect();
        let mut dto = Self {
            id: node.id.to_string(),
            org_id: node.org_id.to_string(),
            host_id: node.host_id.to_string(),
            host_name: node.host_name,
            blockchain_id: node.blockchain_id.to_string(),
            name: node.name,
            address: node.address,
            version: node.version,
            ip: Some(node.ip_addr),
            ip_gateway: node.ip_gateway,
            node_type: 0, // We use the setter to set this field for type-safety
            properties,
            block_height: node.block_height.map(i64::from),
            created_at: Some(super::try_dt_to_ts(node.created_at)?),
            updated_at: Some(super::try_dt_to_ts(node.updated_at)?),
            status: 0,            // We use the setter to set this field for type-safety
            staking_status: None, // We use the setter to set this field for type-safety
            container_status: 0,  // We use the setter to set this field for type-safety
            sync_status: 0,       // We use the setter to set this field for type-safety
            self_update: node.self_update,
            network: node.network,
            blockchain_name: Some(blockchain.name.clone()),
            created_by: user.map(|u| u.id.to_string()),
            created_by_name: user.map(|u| format!("{} {}", u.first_name, u.last_name)),
            created_by_email: user.map(|u| u.email.clone()),
            allow_ips: super::json_value_to_vec(&node.allow_ips)?,
            deny_ips: super::json_value_to_vec(&node.deny_ips)?,
        };
        dto.set_node_type(NodeType::from_model(node.node_type));
        dto.set_status(NodeStatus::from_model(node.chain_status));
        if let Some(ss) = node.staking_status {
            dto.set_staking_status(StakingStatus::from_model(ss));
        }
        dto.set_container_status(ContainerStatus::from_model(node.container_status));
        dto.set_sync_status(SyncStatus::from_model(node.sync_status));
        Ok(dto)
    }
}

impl api::CreateNodeRequest {
    pub fn as_new(&self, user_id: uuid::Uuid) -> crate::Result<models::NewNode<'_>> {
        let properties = models::NodePropertiesWithId {
            id: self.node_type,
            props: models::NodeProperties {
                version: self.version.clone(),
                properties: Some(
                    self.properties
                        .iter()
                        .map(|p| api::node::NodeProperty::into_model(p.clone()))
                        .collect(),
                ),
            },
        };
        Ok(models::NewNode {
            id: uuid::Uuid::new_v4(),
            org_id: self.org_id.parse()?,
            name: petname::Petnames::large().generate_one(3, "_"),
            groups: "".to_string(),
            version: self.version.as_deref(),
            blockchain_id: self.blockchain_id.parse()?,
            properties: serde_json::to_value(properties.props)?,
            block_height: None,
            node_data: None,
            chain_status: models::NodeChainStatus::Provisioning,
            sync_status: models::NodeSyncStatus::Unknown,
            staking_status: models::NodeStakingStatus::Unknown,
            container_status: models::ContainerStatus::Unknown,
            self_update: false,
            vcpu_count: 0,
            mem_size_mb: 0,
            disk_size_gb: 0,
            network: &self.network,
            node_type: properties.id.try_into()?,
            created_by: user_id,
        })
    }
}

impl api::ListNodesRequest {
    fn as_filter(&self) -> crate::Result<models::NodeFilter> {
        Ok(models::NodeFilter {
            org_id: self.org_id.parse()?,
            offset: self.offset,
            limit: self.limit,
            status: self.statuses().map(|s| s.into_model()).collect(),
            node_types: self.node_types().map(|t| t.into_model()).collect(),
            blockchains: self
                .blockchain_ids
                .iter()
                .map(|id| id.parse())
                .collect::<Result<_, _>>()?,
        })
    }
}

impl api::UpdateNodeRequest {
    pub fn as_update(&self) -> crate::Result<models::UpdateNode> {
        Ok(models::UpdateNode {
            id: self.id.parse()?,
            name: None,
            version: None,
            ip_addr: None,
            block_height: None,
            node_data: None,
            chain_status: None,
            sync_status: None,
            staking_status: None,
            container_status: Some(self.container_status().into_model()),
            self_update: self.self_update,
            address: self.address.as_deref(),
        })
    }
}

impl api::node::NodeType {
    pub fn from_model(model: models::NodeType) -> Self {
        match model {
            models::NodeType::Unknown => Self::Unspecified,
            models::NodeType::Miner => Self::Miner,
            models::NodeType::Etl => Self::Etl,
            models::NodeType::Validator => Self::Validator,
            models::NodeType::Api => Self::Api,
            models::NodeType::Oracle => Self::Oracle,
            models::NodeType::Relay => Self::Relay,
            models::NodeType::Execution => Self::Execution,
            models::NodeType::Beacon => Self::Beacon,
            models::NodeType::MevBoost => Self::Mevboost,
            models::NodeType::Node => Self::Node,
            models::NodeType::FullNode => Self::Fullnode,
            models::NodeType::LightNode => Self::Lightnode,
        }
    }

    fn into_model(self) -> models::NodeType {
        match self {
            Self::Unspecified => models::NodeType::Unknown,
            Self::Miner => models::NodeType::Miner,
            Self::Etl => models::NodeType::Etl,
            Self::Validator => models::NodeType::Validator,
            Self::Api => models::NodeType::Api,
            Self::Oracle => models::NodeType::Oracle,
            Self::Relay => models::NodeType::Relay,
            Self::Execution => models::NodeType::Execution,
            Self::Beacon => models::NodeType::Beacon,
            Self::Mevboost => models::NodeType::MevBoost,
            Self::Node => models::NodeType::Node,
            Self::Fullnode => models::NodeType::FullNode,
            Self::Lightnode => models::NodeType::LightNode,
        }
    }
}

impl api::node::ContainerStatus {
    fn from_model(model: models::ContainerStatus) -> Self {
        match model {
            models::ContainerStatus::Unknown => Self::Unspecified,
            models::ContainerStatus::Creating => Self::Creating,
            models::ContainerStatus::Running => Self::Running,
            models::ContainerStatus::Starting => Self::Starting,
            models::ContainerStatus::Stopping => Self::Stopping,
            models::ContainerStatus::Stopped => Self::Stopped,
            models::ContainerStatus::Upgrading => Self::Upgrading,
            models::ContainerStatus::Upgraded => Self::Upgraded,
            models::ContainerStatus::Deleting => Self::Deleting,
            models::ContainerStatus::Deleted => Self::Deleted,
            models::ContainerStatus::Installing => Self::Installing,
            models::ContainerStatus::Snapshotting => Self::Snapshotting,
        }
    }

    fn into_model(self) -> models::ContainerStatus {
        match self {
            Self::Unspecified => models::ContainerStatus::Unknown,
            Self::Creating => models::ContainerStatus::Creating,
            Self::Running => models::ContainerStatus::Running,
            Self::Starting => models::ContainerStatus::Starting,
            Self::Stopping => models::ContainerStatus::Stopping,
            Self::Stopped => models::ContainerStatus::Stopped,
            Self::Upgrading => models::ContainerStatus::Upgrading,
            Self::Upgraded => models::ContainerStatus::Upgraded,
            Self::Deleting => models::ContainerStatus::Deleting,
            Self::Deleted => models::ContainerStatus::Deleted,
            Self::Installing => models::ContainerStatus::Installing,
            Self::Snapshotting => models::ContainerStatus::Snapshotting,
        }
    }
}

impl api::node::NodeStatus {
    fn from_model(model: models::NodeChainStatus) -> Self {
        match model {
            models::NodeChainStatus::Unknown => Self::Unspecified,
            models::NodeChainStatus::Provisioning => Self::Provisioning,
            models::NodeChainStatus::Broadcasting => Self::Broadcasting,
            models::NodeChainStatus::Cancelled => Self::Cancelled,
            models::NodeChainStatus::Delegating => Self::Delegating,
            models::NodeChainStatus::Delinquent => Self::Delinquent,
            models::NodeChainStatus::Disabled => Self::Disabled,
            models::NodeChainStatus::Earning => Self::Earning,
            models::NodeChainStatus::Electing => Self::Electing,
            models::NodeChainStatus::Elected => Self::Elected,
            models::NodeChainStatus::Exported => Self::Exported,
            models::NodeChainStatus::Ingesting => Self::Ingesting,
            models::NodeChainStatus::Mining => Self::Mining,
            models::NodeChainStatus::Minting => Self::Minting,
            models::NodeChainStatus::Processing => Self::Processing,
            models::NodeChainStatus::Relaying => Self::Relaying,
            models::NodeChainStatus::Removed => Self::Removed,
            models::NodeChainStatus::Removing => Self::Removing,
        }
    }

    pub fn into_model(self) -> models::NodeChainStatus {
        match self {
            Self::Unspecified => models::NodeChainStatus::Unknown,
            Self::Provisioning => models::NodeChainStatus::Provisioning,
            Self::Broadcasting => models::NodeChainStatus::Broadcasting,
            Self::Cancelled => models::NodeChainStatus::Cancelled,
            Self::Delegating => models::NodeChainStatus::Delegating,
            Self::Delinquent => models::NodeChainStatus::Delinquent,
            Self::Disabled => models::NodeChainStatus::Disabled,
            Self::Earning => models::NodeChainStatus::Earning,
            Self::Electing => models::NodeChainStatus::Electing,
            Self::Elected => models::NodeChainStatus::Elected,
            Self::Exported => models::NodeChainStatus::Exported,
            Self::Ingesting => models::NodeChainStatus::Ingesting,
            Self::Mining => models::NodeChainStatus::Mining,
            Self::Minting => models::NodeChainStatus::Minting,
            Self::Processing => models::NodeChainStatus::Processing,
            Self::Relaying => models::NodeChainStatus::Relaying,
            Self::Removed => models::NodeChainStatus::Removed,
            Self::Removing => models::NodeChainStatus::Removing,
        }
    }
}

impl api::node::StakingStatus {
    fn from_model(model: models::NodeStakingStatus) -> Self {
        match model {
            models::NodeStakingStatus::Unknown => Self::Unspecified,
            models::NodeStakingStatus::Follower => Self::Follower,
            models::NodeStakingStatus::Staked => Self::Staked,
            models::NodeStakingStatus::Staking => Self::Staking,
            models::NodeStakingStatus::Validating => Self::Validating,
            models::NodeStakingStatus::Consensus => Self::Consensus,
            models::NodeStakingStatus::Unstaked => Self::Unstaked,
        }
    }

    pub fn into_model(self) -> models::NodeStakingStatus {
        match self {
            Self::Unspecified => models::NodeStakingStatus::Unknown,
            Self::Follower => models::NodeStakingStatus::Follower,
            Self::Staked => models::NodeStakingStatus::Staked,
            Self::Staking => models::NodeStakingStatus::Staking,
            Self::Validating => models::NodeStakingStatus::Validating,
            Self::Consensus => models::NodeStakingStatus::Consensus,
            Self::Unstaked => models::NodeStakingStatus::Unstaked,
        }
    }
}

impl api::node::SyncStatus {
    fn from_model(model: models::NodeSyncStatus) -> Self {
        match model {
            models::NodeSyncStatus::Unknown => Self::Unspecified,
            models::NodeSyncStatus::Syncing => Self::Syncing,
            models::NodeSyncStatus::Synced => Self::Synced,
        }
    }

    pub fn into_model(self) -> models::NodeSyncStatus {
        match self {
            Self::Unspecified => models::NodeSyncStatus::Unknown,
            Self::Syncing => models::NodeSyncStatus::Syncing,
            Self::Synced => models::NodeSyncStatus::Synced,
        }
    }
}

impl api::node::NodeProperty {
    fn from_model(model: models::NodePropertyValue) -> Self {
        Self {
            name: model.name,
            label: model.label,
            description: model.description,
            ui_type: model.ui_type,
            disabled: model.disabled,
            required: model.required,
            value: model.value,
        }
    }

    fn into_model(self) -> models::NodePropertyValue {
        models::NodePropertyValue {
            name: self.name,
            label: self.label,
            description: self.description,
            ui_type: self.ui_type,
            disabled: self.disabled,
            required: self.required,
            value: self.value,
        }
    }
}
