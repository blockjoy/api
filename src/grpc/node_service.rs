use crate::grpc::blockjoy::nodes_server::Nodes;
use crate::grpc::blockjoy::NodeInfoUpdateRequest;
use crate::grpc::helpers::required;
use crate::{grpc, models};
use tonic::{Request, Response, Status};

pub struct UpdateNodeServiceImpl {
    db: models::DbPool,
}

impl UpdateNodeServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl Nodes for UpdateNodeServiceImpl {
    async fn info_update(
        &self,
        request: Request<NodeInfoUpdateRequest>,
    ) -> Result<Response<()>, Status> {
        let node_info: grpc::blockjoy::NodeInfo =
            request.into_inner().info.ok_or_else(required("NodeInfo"))?;
        let node_info: models::NodeInfo = node_info.try_into()?;
        let mut tx = self.db.begin().await?;

        models::Node::update_info(&node_info, &mut tx).await?;
        tx.commit().await?;

        Ok(Response::new(()))
    }
}
