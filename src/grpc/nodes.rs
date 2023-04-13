use super::api::{self, nodes_server};
use super::convert;
use super::helpers;
use crate::auth::FindableById;
use crate::{auth, models};
use diesel_async::scoped_futures::ScopedFutureExt;
use futures_util::future::OptionFuture;
use std::collections::HashMap;
use tonic::{Request, Status};

#[tonic::async_trait]
impl nodes_server::Nodes for super::GrpcImpl {
    async fn get(
        &self,
        request: Request<api::GetNodeRequest>,
    ) -> super::Result<api::GetNodeResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, auth::UserAuthToken>(&request)?.clone();
        let inner = request.into_inner();
        let node_id = inner.id.parse().map_err(crate::Error::from)?;
        let mut conn = self.conn().await?;
        let node = models::Node::find_by_id(node_id, &mut conn).await?;

        if node.org_id != token.try_org_id()? {
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

    async fn update(
        &self,
        request: Request<api::UpdateNodeRequest>,
    ) -> super::Result<api::UpdateNodeResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, auth::UserAuthToken>(&request)?;
        let user_id = token.id;

        self.trx(|c| {
            async move {
                let inner = request.into_inner();
                let update_node = inner.as_update()?;
                let user = models::User::find_by_id(user_id, c).await?;
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

    async fn delete(&self, request: Request<api::DeleteNodeRequest>) -> super::Result<()> {
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
        super::response_with_refresh_token(refresh_token, ())
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
        let properties = node
            .properties()?
            .properties
            .into_iter()
            .flatten()
            .map(api::node::NodeProperty::from_model)
            .collect();
        Ok(Self {
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
            r#type: node.node_type.into(),
            properties,
            block_height: node.block_height.map(i64::from),
            created_at: Some(convert::try_dt_to_ts(node.created_at)?),
            updated_at: Some(convert::try_dt_to_ts(node.updated_at)?),
            status: api::node::NodeStatus::from_model(node.chain_status).into(),
            staking_status: node
                .staking_status
                .map(api::node::StakingStatus::from_model)
                .map(Into::into),
            container_status: api::node::ContainerStatus::from_model(node.container_status).into(),
            sync_status: api::node::SyncStatus::from_model(node.sync_status).into(),
            self_update: node.self_update,
            network: node.network,
            blockchain_name: Some(blockchain.name.clone()),
            created_by: user.map(|u| u.id.to_string()),
            created_by_name: user.map(|u| format!("{} {}", u.first_name, u.last_name)),
            created_by_email: user.map(|u| u.email.clone()),
            allow_ips: convert::json_value_to_vec(&node.allow_ips)?,
            deny_ips: convert::json_value_to_vec(&node.deny_ips)?,
        })
    }
}

impl api::ListNodesRequest {
    fn as_filter(&self) -> crate::Result<models::NodeFilter> {
        Ok(models::NodeFilter {
            org_id: self.org_id.parse()?,
            offset: self.offset,
            limit: self.limit,
            status: self
                .status
                .iter()
                .copied()
                .map(models::NodeChainStatus::try_from)
                .collect::<crate::Result<_>>()?,
            node_types: self
                .status
                .iter()
                .copied()
                .map(models::NodeType::try_from)
                .collect::<crate::Result<_>>()?,
            blockchains: self
                .blockchain_id
                .iter()
                .map(|id| id.parse())
                .collect::<Result<_, _>>()?,
        })
    }
}

impl api::CreateNodeRequest {
    pub fn as_new(&self, user_id: uuid::Uuid) -> crate::Result<models::NewNode<'_>> {
        let properties = models::NodePropertiesWithId {
            id: self.r#type,
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
            container_status: self
                .container_status
                .map(models::ContainerStatus::try_from)
                .transpose()?,
            self_update: self.self_update,
            address: self.address.as_deref(),
        })
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
}

impl api::node::SyncStatus {
    fn from_model(model: models::NodeSyncStatus) -> Self {
        match model {
            models::NodeSyncStatus::Unknown => Self::Unspecified,
            models::NodeSyncStatus::Syncing => Self::Syncing,
            models::NodeSyncStatus::Synced => Self::Synced,
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
