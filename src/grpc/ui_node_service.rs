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
use crate::models::{
    Command, CommandRequest, HostCmd, IpAddress, Node, NodeCreateRequest, NodeInfo, User,
    UserSelectiveUpdate,
};
use std::collections::HashMap;
use tonic::{Request, Response, Status};

pub struct NodeServiceImpl {
    db: models::DbPool,
    notifier: Notifier,
}

impl NodeServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self {
            db,
            notifier: Notifier::new(),
        }
    }
}

impl blockjoy_ui::Node {
    /// This function is used to create a ui node from a database node. We want to include the
    /// `database_name` in the ui representation, but it is not in the node model. Therefore we
    /// perform a seperate query to the blockchains table.
    pub async fn from_model(node: models::Node, db: &mut sqlx::PgConnection) -> Result<Self> {
        let blockchain = models::Blockchain::find_by_id(node.blockchain_id, db).await?;
        Self::try_new(node, &blockchain)
    }

    /// This function is used to create many ui nodes from many database nodes. The same
    /// justification as above applies. Note that this function does not simply defer to the
    /// function above, but rather it performs 1 query for n nodes. We like it this way :)
    pub async fn from_models(
        nodes: Vec<models::Node>,
        db: &mut sqlx::PgConnection,
    ) -> Result<Vec<Self>> {
        let blockchain_ids: Vec<_> = nodes.iter().map(|n| n.blockchain_id).collect();
        let blockchains: HashMap<_, _> = models::Blockchain::find_by_ids(&blockchain_ids, db)
            .await?
            .into_iter()
            .map(|b| (b.id, b))
            .collect();

        nodes
            .into_iter()
            .map(|n| (n.blockchain_id, n))
            .map(|(b_id, n)| Self::try_new(n, &blockchains[&b_id]))
            .collect()
    }

    /// Construct a new ui node from the queried parts.
    fn try_new(node: models::Node, blockchain: &models::Blockchain) -> Result<Self> {
        Ok(Self {
            id: Some(node.id.to_string()),
            org_id: Some(node.org_id.to_string()),
            host_id: Some(node.host_id.to_string()),
            host_name: Some(node.host_name),
            blockchain_id: Some(node.blockchain_id.to_string()),
            name: node.name,
            // TODO: get node groups
            groups: vec![],
            version: node.version,
            ip: node.ip_addr,
            ip_gateway: node.ip_gateway,
            r#type: Some(node.node_type.to_json()?),
            address: node.address,
            wallet_address: node.wallet_address,
            block_height: node.block_height.map(i64::from),
            // TODO: Get node data
            node_data: None,
            created_at: Some(convert::try_dt_to_ts(node.created_at)?),
            updated_at: Some(convert::try_dt_to_ts(node.updated_at)?),
            status: Some(blockjoy_ui::node::NodeStatus::from(node.chain_status).into()),
            staking_status: Some(
                blockjoy_ui::node::StakingStatus::from(node.staking_status).into(),
            ),
            sync_status: Some(blockjoy_ui::node::SyncStatus::from(node.sync_status).into()),
            self_update: Some(node.self_update),
            network: Some(node.network),
            blockchain_name: Some(blockchain.name.clone()),
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
            .unwrap_or(&"".to_string())
            .to_owned();
        let inner = request.into_inner();
        let node_id = inner.id.parse().map_err(ApiError::from)?;
        let mut conn = self.db.conn().await?;
        let node = Node::find_by_id(node_id, &mut conn).await?;

        if node.org_id.to_string() == org_id {
            let response = GetNodeResponse {
                meta: Some(ResponseMeta::from_meta(inner.meta, Some(token.try_into()?))),
                node: Some(blockjoy_ui::Node::from_model(node, &mut conn).await?),
            };
            Ok(response_with_refresh_token(refresh_token, response)?)
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
                Node::find_all_by_org(org_id, offset, pagination.items_per_page, &mut conn).await?
            }
            Some(filter) => {
                let filter = filter
                    .try_into()
                    .map_err(|_| Status::internal("Unexpected error at filtering"))?;

                Node::find_all_by_filter(
                    org_id,
                    filter,
                    offset,
                    pagination.items_per_page,
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
        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    async fn create(
        &self,
        request: Request<CreateNodeRequest>,
    ) -> Result<Response<CreateNodeResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        // Check quota
        let mut conn = self.db.conn().await?;
        let user = User::find_by_id(token.id, &mut conn).await?;

        if user.staking_quota <= 0 {
            return Err(Status::resource_exhausted("User node quota exceeded"));
        }

        let inner = request.into_inner();
        let mut fields: NodeCreateRequest = inner.node.ok_or_else(required("node"))?.try_into()?;
        let mut tx = self.db.begin().await?;
        let node = Node::create(&mut fields, &mut tx).await?;

        self.notifier
            .bv_nodes_sender()
            .send(&node.clone().into())
            .await?;
        self.notifier
            .ui_nodes_sender()
            .send(&node.clone().try_into()?)
            .await?;

        let req = CommandRequest {
            cmd: HostCmd::CreateNode,
            sub_cmd: None,
            resource_id: node.id,
        };
        let cmd = Command::create(node.host_id, req, &mut tx).await?;
        let grpc_cmd = convert::db_command_to_grpc_command(&cmd, &mut tx).await?;
        self.notifier.bv_commands_sender().send(&grpc_cmd).await?;

        let update_user = UserSelectiveUpdate {
            first_name: None,
            last_name: None,
            fee_bps: None,
            staking_quota: Some(user.staking_quota - 1),
            refresh_token: None,
        };
        User::update_all(user.id, update_user, &mut tx).await?;
        let req = CommandRequest {
            cmd: HostCmd::RestartNode,
            sub_cmd: None,
            resource_id: node.id,
        };
        let cmd = Command::create(node.host_id, req, &mut tx).await?;
        let grpc_cmd = convert::db_command_to_grpc_command(&cmd, &mut tx).await?;
        self.notifier.bv_commands_sender().send(&grpc_cmd).await?;

        tx.commit().await?;

        let response_meta =
            ResponseMeta::from_meta(inner.meta, Some(token.try_into()?)).with_message(node.id);
        let response = CreateNodeResponse {
            meta: Some(response_meta),
        };

        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    async fn update(
        &self,
        request: Request<UpdateNodeRequest>,
    ) -> Result<Response<UpdateNodeResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        let node = inner.node.ok_or_else(required("node"))?;
        let fields: NodeInfo = node.try_into()?;

        let mut tx = self.db.begin().await?;
        Node::update_info(&fields, &mut tx).await?;
        tx.commit().await?;
        let response = UpdateNodeResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
        };
        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    async fn delete(&self, request: Request<DeleteNodeRequest>) -> Result<Response<()>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = request
            .extensions()
            .get::<UserAuthToken>()
            .ok_or_else(required("User token"))?
            .clone();
        let inner = request.into_inner();
        let node_id = inner.id.parse().map_err(ApiError::from)?;
        let mut conn = self.db.conn().await?;
        let mut tx = self.db.begin().await?;
        let node = Node::find_by_id(node_id, &mut tx).await?;

        if Node::belongs_to_user_org(node.org_id, token.id, &mut tx).await? {
            // 1. Delete node, if the node belongs to the current user
            // Key files are deleted automatically because of 'on delete cascade' in tables DDL
            Node::delete(node_id, &mut tx).await?;

            let host_id = node.host_id;
            // 2. Do NOT delete reserved IP addresses, but set assigned to false
            let ip = IpAddress::find_by_node(node.ip_addr.unwrap_or_default(), &mut tx).await?;

            IpAddress::unassign(ip.id, host_id, &mut tx).await?;

            // Send delete node command
            let req = CommandRequest {
                cmd: HostCmd::DeleteNode,
                sub_cmd: None,
                resource_id: node_id,
            };
            let cmd = Command::create(node.host_id, req, &mut tx).await?;
            let user_id = token.id;
            let user = User::find_by_id(user_id, &mut conn).await?;
            let update_user = UserSelectiveUpdate {
                first_name: None,
                last_name: None,
                fee_bps: None,
                staking_quota: Some(user.staking_quota + 1),
                refresh_token: None,
            };

            User::update_all(user_id, update_user, &mut tx).await?;

            let grpc_cmd = convert::db_command_to_grpc_command(&cmd, &mut tx).await?;

            tx.commit().await?;

            self.notifier.bv_commands_sender().send(&grpc_cmd).await?;
            // let grpc_cmd = cmd.clone().try_into()?;
            // self.notifier.ui_commands_sender().send(&grpc_cmd).await;

            Ok(response_with_refresh_token::<()>(refresh_token, ())?)
        } else {
            Err(Status::permission_denied("User cannot delete node"))
        }
    }
}
