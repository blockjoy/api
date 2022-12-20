use super::helpers::{internal, required};
use crate::auth::{FindableById, UserAuthToken};
use crate::errors::ApiError;
use crate::grpc::blockjoy_ui::node_service_server::NodeService;
use crate::grpc::blockjoy_ui::{
    CreateNodeRequest, CreateNodeResponse, DeleteNodeRequest, GetNodeRequest, GetNodeResponse,
    ListNodesRequest, ListNodesResponse, Node as GrpcNode, ResponseMeta, UpdateNodeRequest,
    UpdateNodeResponse,
};
use crate::grpc::notification::{ChannelNotification, ChannelNotifier, NotificationPayload};
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::models::{
    Command, CommandRequest, Host, HostCmd, IpAddress, Node, NodeCreateRequest, NodeInfo,
    NodeKeyFile,
};
use crate::server::DbPool;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct NodeServiceImpl {
    db: DbPool,
    notifier: Arc<ChannelNotifier>,
}

impl NodeServiceImpl {
    pub fn new(db: DbPool, notifier: Arc<ChannelNotifier>) -> Self {
        Self { db, notifier }
    }
}

#[tonic::async_trait]
impl NodeService for NodeServiceImpl {
    async fn get(
        &self,
        request: Request<GetNodeRequest>,
    ) -> Result<Response<GetNodeResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let inner = request.into_inner();
        let node_id = Uuid::parse_str(inner.id.as_str()).map_err(ApiError::from)?;
        let node = Node::find_by_id(node_id, &self.db).await?;
        let response = GetNodeResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta)),
            node: Some(node.try_into()?),
        };
        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    async fn list(
        &self,
        request: Request<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let inner = request.into_inner();
        let filters = inner.filter.clone();
        let org_id = Uuid::parse_str(inner.org_id.as_str()).map_err(ApiError::from)?;
        let pagination = inner
            .meta
            .clone()
            .ok_or_else(|| Status::invalid_argument("Metadata missing"))?;
        let pagination = pagination
            .pagination
            .ok_or_else(|| Status::invalid_argument("Pagination missing"))?;
        let offset = pagination.items_per_page * (pagination.current_page - 1);

        let nodes = match filters {
            None => {
                Node::find_all_by_org(org_id, offset, pagination.items_per_page, &self.db).await?
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
                    &self.db,
                )
                .await?
            }
        };

        let nodes: Result<_, ApiError> = nodes.iter().map(GrpcNode::try_from).collect();
        let response = ListNodesResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta)),
            nodes: nodes?,
        };
        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    async fn create(
        &self,
        request: Request<CreateNodeRequest>,
    ) -> Result<Response<CreateNodeResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let inner = request.into_inner();
        let mut fields: NodeCreateRequest = inner.node.ok_or_else(required("node"))?.try_into()?;
        let host = Host::find_by_id(fields.host_id, &self.db).await?;
        // Set IPs retrieved from system
        fields.ip_gateway = host.ip_gateway.map(|ip| ip.to_string());
        fields.ip_addr = Some(
            IpAddress::next_for_host(fields.host_id, &self.db)
                .await?
                .ip
                .to_string(),
        );

        let node = Node::create(&fields, &self.db).await?;
        let req = CommandRequest {
            cmd: HostCmd::CreateNode,
            sub_cmd: None,
            resource_id: node.id,
        };

        let cmd = Command::create(fields.host_id, req, &self.db).await?;
        let payload = NotificationPayload::new(cmd.id);
        let notification = ChannelNotification::Command(payload);

        // Sending commands receiver (in command_flow.rs)
        self.notifier
            .commands_sender()
            .send(notification)
            .map_err(internal)?;
        let payload = NotificationPayload::new(node.id);
        let notification = ChannelNotification::Node(payload);

        // Sending notification to nodes receiver (in ui_update_service.rs)
        self.notifier
            .nodes_sender()
            .send(notification)
            .map_err(internal)?;
        let response_meta = ResponseMeta::from_meta(inner.meta).with_message(node.id);
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
        let inner = request.into_inner();
        let node = inner.node.ok_or_else(required("node"))?;
        let node_id = node.id.as_deref();
        let node_id = node_id
            .ok_or_else(required("node.id"))?
            .parse()
            .map_err(ApiError::from)?;
        let fields: NodeInfo = node.try_into()?;

        Node::update_info(&node_id, &fields, &self.db).await?;
        let response = UpdateNodeResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta)),
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
        let node = Node::find_by_id(node_id, &self.db).await?;

        if Node::belongs_to_user_org(node.org_id, *token.id(), &self.db).await? {
            // 1. Delete node, if the node belongs to the current user
            Node::delete(node_id, &self.db).await?;

            // 2. Delete all key files
            let key_files = NodeKeyFile::find_by_node(node_id, &self.db).await?;

            for key in key_files {
                NodeKeyFile::delete(key.id, &self.db).await?;
            }

            // 3. Delete reserved IP addresses
            let ips = IpAddress::find_by_node(node_id, &self.db).await?;

            for ip in ips {
                IpAddress::delete(ip.id, &self.db).await?;
            }

            // Send delete node command
            let req = CommandRequest {
                cmd: HostCmd::DeleteNode,
                sub_cmd: None,
                resource_id: node_id,
            };
            let del_cmd = Command::create(node.host_id, req, &self.db).await?;
            let payload = NotificationPayload::new(del_cmd.id);
            let notification = ChannelNotification::Command(payload);

            // Sending commands receiver (in command_flow.rs)
            self.notifier
                .commands_sender()
                .send(notification)
                .map_err(internal)?;
            let payload = NotificationPayload::new(node.id);
            let notification = ChannelNotification::Node(payload);

            // Sending notification to nodes receiver (in ui_update_service.rs)
            self.notifier
                .nodes_sender()
                .send(notification)
                .map_err(internal)?;

            Ok(response_with_refresh_token::<()>(refresh_token, ())?)
        } else {
            Err(Status::permission_denied("User cannot delete node"))
        }
    }
}
