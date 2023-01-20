use crate::auth::FindableById;
use crate::grpc::blockjoy_ui::update_notification::Notification;
use crate::grpc::blockjoy_ui::update_service_server::UpdateService;
use crate::grpc::blockjoy_ui::{self, GetUpdatesRequest, GetUpdatesResponse};
use crate::models;
use crate::models::{Host, Node};
use std::pin::Pin;
use tonic::codegen::futures_core::Stream;
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct UpdateServiceImpl {
    _db: models::DbPool,
}

impl UpdateServiceImpl {
    pub fn new(_db: models::DbPool) -> Self {
        Self { _db }
    }

    pub async fn host_payload(id: Uuid, db: models::DbPool) -> Option<Notification> {
        let mut conn = db.conn().await.ok()?;
        let host = Host::find_by_id(id, &mut conn)
            .await
            .map_err(|e| tracing::error!("Host ID {id} not found: {e}"))
            .ok()?;
        let host = blockjoy_ui::Host::from_model(host, &mut conn).await.ok()?;
        Some(Notification::Host(host))
    }

    pub async fn node_payload(id: Uuid, db: models::DbPool) -> Option<Notification> {
        let mut conn = db.conn().await.ok()?;
        let node = Node::find_by_id(id, &mut conn)
            .await
            .map_err(|e| tracing::error!("Node ID {id} not found: {e}"))
            .ok()?;
        let node = blockjoy_ui::Node::from_model(node, &mut conn).await.ok()?;
        Some(Notification::Node(node))
    }
}

#[tonic::async_trait]
impl UpdateService for UpdateServiceImpl {
    type UpdatesStream =
        Pin<Box<dyn Stream<Item = Result<GetUpdatesResponse, Status>> + Send + 'static>>;

    async fn updates(
        &self,
        _request: Request<GetUpdatesRequest>,
    ) -> Result<Response<Self::UpdatesStream>, Status> {
        todo!()
    }
}
