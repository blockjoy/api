use crate::grpc::blockjoy;
use crate::grpc::blockjoy::node_service_server::NodeService;
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};

impl blockjoy::NodeUpdateRequest {
    pub fn as_update(&self) -> crate::Result<models::UpdateNode> {
        Ok(models::UpdateNode {
            id: self.id.parse()?,
            name: None,
            version: None,
            ip_addr: self.ip.as_deref(),
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
        })
    }
}

impl blockjoy::Node {
    pub fn from_model(model: models::Node) -> Self {
        Self {
            id: model.id.to_string(),
            name: model.name,
            ip: model.ip_addr,
            self_update: model.self_update,
            block_height: model.block_height,
            onchain_name: None,
            app_status: model.chain_status as i32,
            container_status: Some(model.container_status as i32),
            sync_status: Some(model.sync_status as i32),
            staking_status: model.staking_status.map(|ss| ss as i32),
            address: model.address,
            host_id: model.host_id.to_string(),
        }
    }
}

#[tonic::async_trait]
impl NodeService for super::GrpcImpl {
    async fn update(
        &self,
        request: Request<blockjoy::NodeUpdateRequest>,
    ) -> Result<Response<()>, Status> {
        let request = request.into_inner();
        let update = request.as_update()?;
        self.db.trx(|c| update.update(c).scope_boxed()).await?;

        Ok(Response::new(()))
    }
}
