use super::blockjoy;
use super::helpers::required;
use super::notification::Notifier;
use crate::auth::{FindableById, UserAuthToken};
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy_ui::node_service_server::NodeService;
use crate::grpc::blockjoy_ui::{
    self, CreateNodeRequest, CreateNodeResponse, DeleteNodeRequest, GetNodeRequest,
    GetNodeResponse, ListNodesRequest, ListNodesResponse, ResponseMeta, UpdateNodeRequest,
    UpdateNodeResponse,
};
use crate::grpc::helpers::try_get_token;
use crate::grpc::{convert, get_refresh_token, response_with_refresh_token};
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use std::collections::HashMap;
use tonic::{Request, Response, Status};

pub struct NodeServiceImpl {
    db: models::DbPool,
    notifier: Notifier,
}

impl NodeServiceImpl {
    pub fn new(db: models::DbPool, notifier: Notifier) -> Self {
        Self { db, notifier }
    }
}

impl blockjoy_ui::Node {
    /// This function is used to create a ui node from a database node. We want to include the
    /// `database_name` in the ui representation, but it is not in the node model. Therefore we
    /// perform a seperate query to the blockchains table.
    pub async fn from_model(
        node: models::Node,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> Result<Self> {
        let blockchain = models::Blockchain::find_by_id(node.blockchain_id, conn).await?;
        Self::new(node, &blockchain)
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

        nodes
            .into_iter()
            .map(|n| (n.blockchain_id, n))
            .map(|(b_id, n)| Self::new(n, &blockchains[&b_id]))
            .collect()
    }

    /// Construct a new ui node from the queried parts.
    fn new(node: models::Node, blockchain: &models::Blockchain) -> Result<Self> {
        let node_type = node.node_type()?;
        Ok(Self {
            id: Some(node.id.to_string()),
            org_id: Some(node.org_id.to_string()),
            host_id: Some(node.host_id.to_string()),
            host_name: Some(node.host_name),
            blockchain_id: Some(node.blockchain_id.to_string()),
            name: Some(node.name),
            // TODO: get node groups
            groups: vec![],
            version: node.version,
            ip: node.ip_addr,
            ip_gateway: Some(node.ip_gateway),
            r#type: Some(serde_json::to_string(&node_type)?),
            address: node.address,
            wallet_address: node.wallet_address,
            block_height: node.block_height.map(i64::from),
            // TODO: Get node data
            node_data: None,
            created_at: Some(convert::try_dt_to_ts(node.created_at)?),
            updated_at: Some(convert::try_dt_to_ts(node.updated_at)?),
            status: Some(blockjoy_ui::node::NodeStatus::from(node.chain_status).into()),
            staking_status: node
                .staking_status
                .map(blockjoy_ui::node::StakingStatus::from)
                .map(Into::into),
            sync_status: Some(blockjoy_ui::node::SyncStatus::from(node.sync_status).into()),
            self_update: Some(node.self_update),
            network: Some(node.network),
            blockchain_name: Some(blockchain.name.clone()),
        })
    }

    pub fn as_new(&self) -> Result<models::NewNode<'_>> {
        Ok(models::NewNode {
            id: uuid::Uuid::new_v4(),
            org_id: self
                .org_id
                .as_ref()
                .ok_or_else(required("node.org_id"))?
                .parse()?,
            host_name: self.host_name.as_deref(),
            name: petname::petname(3, "_"),
            groups: self.groups.join(","),
            version: self.version.as_deref(),
            ip_addr: self.ip.as_deref(),
            ip_gateway: self.ip_gateway.as_deref(),
            blockchain_id: self
                .blockchain_id
                .as_ref()
                .ok_or_else(required("node.blockchain_id"))?
                .parse()?,
            node_type: serde_json::from_str(
                self.r#type.as_ref().ok_or_else(required("node.type"))?,
            )?,
            address: self.address.as_deref(),
            wallet_address: self.wallet_address.as_deref(),
            block_height: self.block_height,
            node_data: self
                .node_data
                .as_deref()
                .map(serde_json::from_slice)
                .transpose()?,
            chain_status: self
                .status
                .ok_or_else(required("node.status"))?
                .try_into()?,
            sync_status: models::NodeSyncStatus::Unknown,
            staking_status: models::NodeStakingStatus::Unknown,
            container_status: models::ContainerStatus::Unknown,
            self_update: self.self_update.unwrap_or(false),
            vcpu_count: 0,
            mem_size_mb: 0,
            disk_size_gb: 0,
            network: self
                .network
                .as_deref()
                .ok_or_else(required("node.network"))?,
        })
    }

    fn as_update(&self) -> Result<models::UpdateNode<'_>> {
        Ok(models::UpdateNode {
            id: self.id.as_ref().ok_or_else(required("node.id"))?.parse()?,
            name: self.name.as_deref(),
            version: self.version.as_deref(),
            ip_addr: self.ip.as_deref(),
            block_height: None,
            node_data: self
                .node_data
                .as_deref()
                .map(serde_json::from_slice)
                .transpose()?,
            chain_status: None,
            sync_status: None,
            staking_status: None,
            container_status: None,
            self_update: self.self_update,
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
impl NodeService for NodeServiceImpl {
    async fn get(
        &self,
        request: Request<GetNodeRequest>,
    ) -> Result<Response<GetNodeResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let org_id = token
            .data
            .get("org_id")
            .ok_or_else(required("token.org_id"))?
            .to_owned();
        let inner = request.into_inner();
        let node_id = inner.id.parse().map_err(ApiError::from)?;
        let mut conn = self.db.conn().await?;
        let node = models::Node::find_by_id(node_id, &mut conn).await?;

        if node.org_id.to_string() == org_id {
            let response = GetNodeResponse {
                meta: Some(ResponseMeta::from_meta(inner.meta, Some(token.try_into()?))),
                node: Some(blockjoy_ui::Node::from_model(node, &mut conn).await?),
            };
            response_with_refresh_token(refresh_token, response)
        } else {
            Err(Status::permission_denied("Access not allowed"))
        }
    }

    async fn list(
        &self,
        request: Request<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        let filters = inner.filter.clone();
        let org_id = inner.org_id.parse().map_err(ApiError::from)?;
        let pagination = inner
            .meta
            .clone()
            .ok_or_else(|| Status::invalid_argument("Metadata missing"))?;
        let pagination = pagination
            .pagination
            .ok_or_else(|| Status::invalid_argument("Pagination missing"))?;
        let offset = pagination.items_per_page * (pagination.current_page - 1);

        let mut conn = self.db.conn().await?;
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
        let mut conn = self.db.conn().await?;
        let user = models::User::find_by_id(token.id, &mut conn).await?;

        if user.staking_quota <= 0 {
            return Err(Status::resource_exhausted("User node quota exceeded"));
        }

        let inner = request.into_inner();
        let node = self
            .db
            .trx(|c| {
                async move {
                    let node = inner.node.as_ref().ok_or_else(required("node"))?;
                    let node = node.as_new()?.create(c).await?;

                    self.notifier
                        .bv_nodes_sender()?
                        .send(&blockjoy::NodeInfo::from_model(node.clone()))
                        .await?;
                    self.notifier
                        .ui_nodes_sender()?
                        .send(&node.clone().try_into()?)
                        .await?;

                    let new_command = models::NewCommand {
                        host_id: node.host_id,
                        cmd: models::HostCmd::CreateNode,
                        sub_cmd: None,
                        resource_id: node.id,
                    };
                    let cmd = new_command.create(c).await?;
                    let grpc_cmd = convert::db_command_to_grpc_command(&cmd, c).await?;
                    self.notifier.bv_commands_sender()?.send(&grpc_cmd).await?;

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
                        resource_id: node.id,
                    };
                    let cmd = new_command.create(c).await?;
                    let grpc_cmd = convert::db_command_to_grpc_command(&cmd, c).await?;
                    self.notifier.bv_commands_sender()?.send(&grpc_cmd).await?;
                    Ok(node)
                }
                .scope_boxed()
            })
            .await?;

        let response_meta =
            ResponseMeta::from_meta(inner.meta, Some(token.try_into()?)).with_message(node.id);
        let response = CreateNodeResponse {
            meta: Some(response_meta),
        };

        response_with_refresh_token(refresh_token, response)
    }

    async fn update(
        &self,
        request: Request<UpdateNodeRequest>,
    ) -> Result<Response<UpdateNodeResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        let update_node = inner
            .node
            .as_ref()
            .ok_or_else(required("node"))?
            .as_update()?;

        self.db
            .trx(|c| {
                async move {
                    // Check if the node exists
                    models::Node::find_by_id(update_node.id, c).await?;
                    update_node.update(c).await
                }
                .scope_boxed()
            })
            .await?;
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
        self.db
            .trx(|c| {
                async move {
                    let node_id = inner.id.parse()?;
                    let node = models::Node::find_by_id(node_id, c).await?;

                    if !models::Node::belongs_to_user_org(node.org_id, token.id, c).await? {
                        return Err(Status::permission_denied("User cannot delete node").into());
                    }
                    // 1. Delete node, if the node belongs to the current user
                    // Key files are deleted automatically because of 'on delete cascade' in tables DDL
                    models::Node::delete(node_id, c).await?;

                    let host_id = node.host_id;
                    // 2. Do NOT delete reserved IP addresses, but set assigned to false
                    let ip_addr = dbg!(node.ip_addr.as_ref())
                        .ok_or_else(required("node.ip_addr"))?
                        .parse()
                        .map_err(|_| Status::internal("invalid ip"))?;
                    let ip = dbg!(models::IpAddress::find_by_node(ip_addr, c).await)?;

                    dbg!(models::IpAddress::unassign(ip.id, host_id, c).await)?;

                    // Send delete node command
                    let new_command = models::NewCommand {
                        host_id: node.host_id,
                        cmd: models::HostCmd::DeleteNode,
                        sub_cmd: None,
                        resource_id: node_id,
                    };
                    let cmd = dbg!(new_command.create(c).await)?;
                    let user_id = token.id;
                    let user = dbg!(models::User::find_by_id(user_id, c).await)?;
                    let update_user = models::UpdateUser {
                        id: user.id,
                        first_name: None,
                        last_name: None,
                        fee_bps: None,
                        staking_quota: Some(user.staking_quota + 1),
                        refresh: None,
                    };

                    dbg!(update_user.update(c).await)?;

                    let grpc_cmd = dbg!(convert::db_command_to_grpc_command(&cmd, c).await)?;

                    self.notifier.bv_commands_sender()?.send(&grpc_cmd).await
                    // let grpc_cmd = cmd.clone().try_into()?;
                    // self.notifier.ui_commands_sender()?.send(&grpc_cmd).await;
                }
                .scope_boxed()
            })
            .await?;
        Ok(response_with_refresh_token::<()>(refresh_token, ())?)
    }
}
