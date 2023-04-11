use super::blockjoy;
use super::helpers::required;
use crate::auth::{FindableById, UserAuthToken};
use crate::grpc::blockjoy_ui::node_service_server::NodeService;
use crate::grpc::blockjoy_ui::{
    self, CreateNodeRequest, CreateNodeResponse, DeleteNodeRequest, GetNodeRequest,
    GetNodeResponse, ListNodesRequest, ListNodesResponse, ResponseMeta, UpdateNodeRequest,
    UpdateNodeResponse,
};
use crate::grpc::convert::json_value_to_vec;
use crate::grpc::helpers::try_get_token;
use crate::grpc::{convert, get_refresh_token, response_with_refresh_token};
use crate::models;
use crate::Result;
use diesel_async::scoped_futures::ScopedFutureExt;
use futures_util::future::OptionFuture;
use std::collections::HashMap;
use tonic::{Request, Response, Status};

impl blockjoy_ui::Node {
    /// This function is used to create a ui node from a database node. We want to include the
    /// `database_name` in the ui representation, but it is not in the node model. Therefore we
    /// perform a seperate query to the blockchains table.
    pub async fn from_model(
        node: models::Node,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> Result<Self> {
        let blockchain = models::Blockchain::find_by_id(node.blockchain_id, conn).await?;
        let user = OptionFuture::from(
            node.created_by
                .map(|u_id| models::User::find_by_id(u_id, conn)),
        )
        .await
        .transpose()?;
        Self::new(node, &blockchain, user.as_ref())
    }

    /// This function is used to create many ui nodes from many database nodes. The same
    /// justification as above applies. Note that this function does not simply defer to the
    /// function above, but rather it performs 1 query for n nodes. We like it this way :)
    pub async fn from_models(
        nodes: Vec<models::Node>,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> Result<Vec<Self>> {
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
    ) -> Result<Self> {
        let properties = node
            .properties()?
            .properties
            .into_iter()
            .flatten()
            .map(blockjoy_ui::node::NodeProperty::from_model)
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
            status: blockjoy_ui::node::NodeStatus::from(node.chain_status).into(),
            staking_status: node
                .staking_status
                .map(blockjoy_ui::node::StakingStatus::from)
                .map(Into::into),
            sync_status: blockjoy_ui::node::SyncStatus::from(node.sync_status).into(),
            self_update: node.self_update,
            network: node.network,
            blockchain_name: Some(blockchain.name.clone()),
            created_by: user.map(|u| u.id.to_string()),
            created_by_name: user.map(|u| format!("{} {}", u.first_name, u.last_name)),
            created_by_email: user.map(|u| u.email.clone()),
            allow_ips: json_value_to_vec(&node.allow_ips)?,
            deny_ips: json_value_to_vec(&node.deny_ips)?,
        })
    }
}

impl blockjoy_ui::node::NodeProperty {
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

impl blockjoy_ui::CreateNodeRequest {
    pub fn as_new(&self, user_id: uuid::Uuid) -> Result<models::NewNode<'_>> {
        let properties = models::NodePropertiesWithId {
            id: self.r#type,
            props: models::NodeProperties {
                version: self.version.clone(),
                properties: Some(
                    self.properties
                        .iter()
                        .map(|p| blockjoy_ui::node::NodeProperty::into_model(p.clone()))
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

impl blockjoy_ui::UpdateNodeRequest {
    /// This function is currently a stub, since the front end never updates nodes. We might need to
    /// figure out which field we want to be updatable.
    fn as_update(&self) -> Result<models::UpdateNode<'_>> {
        Ok(models::UpdateNode {
            id: self.id.parse()?,
            // Updating node names is not allowed, this would make Alexey extremely sad.
            name: None,
            version: self.version.as_deref(),
            ip_addr: None,
            block_height: None,
            node_data: None,
            chain_status: None,
            sync_status: None,
            staking_status: None,
            container_status: None,
            self_update: None,
            address: None,
        })
    }
}

impl blockjoy_ui::FilterCriteria {
    fn as_model(&self) -> Result<models::NodeFilter> {
        Ok(models::NodeFilter {
            status: self
                .states
                .iter()
                .map(|status| status.parse())
                .collect::<crate::Result<_>>()?,
            node_types: self
                .node_types
                .iter()
                .map(|id| id.parse())
                .collect::<Result<_, _>>()?,
            blockchains: self
                .blockchain_ids
                .iter()
                .map(|id| id.parse())
                .collect::<Result<_, _>>()?,
        })
    }
}

#[tonic::async_trait]
impl NodeService for super::GrpcImpl {
    async fn get(
        &self,
        request: Request<GetNodeRequest>,
    ) -> Result<Response<GetNodeResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let inner = request.into_inner();
        let node_id = inner.id.parse().map_err(crate::Error::from)?;
        let mut conn = self.conn().await?;
        let node = models::Node::find_by_id(node_id, &mut conn).await?;

        if node.org_id != token.try_org_id()? {
            super::bail_unauthorized!("Access not allowed")
        }
        let response = GetNodeResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token.try_into()?))),
            node: Some(blockjoy_ui::Node::from_model(node, &mut conn).await?),
        };
        response_with_refresh_token(refresh_token, response)
    }

    async fn list(
        &self,
        request: Request<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        let filters = inner.filter.clone();
        let org_id = inner.org_id.parse().map_err(crate::Error::from)?;
        let pagination = inner
            .meta
            .clone()
            .ok_or_else(|| Status::invalid_argument("Metadata missing"))?;
        let pagination = pagination
            .pagination
            .ok_or_else(|| Status::invalid_argument("Pagination missing"))?;
        let offset = pagination.items_per_page * (pagination.current_page - 1);

        let mut conn = self.conn().await?;
        let nodes = match filters {
            None => {
                models::Node::find_all_by_org(
                    org_id,
                    offset.into(),
                    pagination.items_per_page.into(),
                    &mut conn,
                )
                .await?
            }
            Some(filter) => {
                let filter = filter.as_model()?;

                models::Node::find_all_by_filter(
                    org_id,
                    filter,
                    offset.into(),
                    pagination.items_per_page.into(),
                    &mut conn,
                )
                .await?
            }
        };

        let nodes = blockjoy_ui::Node::from_models(nodes, &mut conn).await?;
        let response = ListNodesResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
            nodes,
        };
        response_with_refresh_token(refresh_token, response)
    }

    async fn create(
        &self,
        request: Request<CreateNodeRequest>,
    ) -> Result<Response<CreateNodeResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        // Check quota
        let mut conn = self.conn().await?;
        let user = models::User::find_by_id(token.id, &mut conn).await?;

        if user.staking_quota <= 0 {
            return Err(Status::resource_exhausted("User node quota exceeded"));
        }

        let inner = request.into_inner();
        let new_node = inner.as_new(user.id)?;
        let (node, ui_node, node_msg, create_msg, restart_msg) = self
            .trx(|c| {
                async move {
                    let node = new_node.create(c).await?;

                    let node_msg = blockjoy_ui::NodeMessage::created(node.clone(), c).await?;

                    let new_command = models::NewCommand {
                        host_id: node.host_id,
                        cmd: models::HostCmd::CreateNode,
                        sub_cmd: None,
                        node_id: Some(node.id),
                    };
                    let cmd = new_command.create(c).await?;
                    let create_msg = blockjoy::Command::from_model(&cmd, c).await?;

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
                    let cmd = new_command.create(c).await?;
                    let restart_msg = blockjoy::Command::from_model(&cmd, c).await?;
                    let ui_node = blockjoy_ui::Node::from_model(node.clone(), c).await?;
                    Ok((node, ui_node, node_msg, create_msg, restart_msg))
                }
                .scope_boxed()
            })
            .await?;

        self.notifier
            .bv_nodes_sender()?
            .send(&blockjoy::Node::from_model(node.clone()))
            .await?;
        self.notifier.ui_nodes_sender()?.send(&node_msg).await?;
        self.notifier
            .bv_commands_sender()?
            .send(&create_msg)
            .await?;
        self.notifier
            .bv_commands_sender()?
            .send(&restart_msg)
            .await?;
        let response_meta =
            ResponseMeta::from_meta(inner.meta, Some(token.try_into()?)).with_message(node.id);
        let response = CreateNodeResponse {
            meta: Some(response_meta),
            node: Some(ui_node),
        };

        response_with_refresh_token(refresh_token, response)
    }

    async fn update(
        &self,
        request: Request<UpdateNodeRequest>,
    ) -> Result<Response<UpdateNodeResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?;
        let user_id = token.id;
        let token = token.try_into()?;
        let inner = request.into_inner();
        let update_node = inner.as_update()?;

        let msg = self
            .trx(|c| {
                async move {
                    let user = models::User::find_by_id(user_id, c).await?;
                    let node = update_node.update(c).await?;
                    blockjoy_ui::NodeMessage::updated(node, user, c).await
                }
                .scope_boxed()
            })
            .await?;

        self.notifier.ui_nodes_sender()?.send(&msg).await?;

        let response = UpdateNodeResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
        };
        response_with_refresh_token(refresh_token, response)
    }

    async fn delete(&self, request: Request<DeleteNodeRequest>) -> Result<Response<()>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = request
            .extensions()
            .get::<UserAuthToken>()
            .ok_or_else(required("User token"))?
            .clone();
        let inner = request.into_inner();
        self.trx(|c| {
            async move {
                let node_id = inner.id.parse()?;
                let node = models::Node::find_by_id(node_id, c).await?;

                if !models::Node::belongs_to_user_org(node.org_id, token.id, c).await? {
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

                let user_id = token.id;
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

                let grpc_cmd = blockjoy::Command::from_model(&cmd, c).await?;
                self.notifier.bv_commands_sender()?.send(&grpc_cmd).await?;

                self.notifier
                    .ui_nodes_sender()?
                    .send(&blockjoy_ui::NodeMessage::deleted(node, user))
                    .await?;
                Ok(())
            }
            .scope_boxed()
        })
        .await?;
        response_with_refresh_token(refresh_token, ())
    }
}
