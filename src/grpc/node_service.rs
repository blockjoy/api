use crate::grpc::blockjoy;
use crate::grpc::blockjoy::nodes_server::Nodes;
use crate::grpc::helpers::required;
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};

impl blockjoy::NodeInfo {
    pub fn as_update(&self) -> crate::Result<models::UpdateNode> {
        Ok(models::UpdateNode {
            id: self.id.parse()?,
            name: self.name.as_deref(),
            version: None,
            ip_addr: self.ip.as_deref(),
            block_height: self.block_height,
            node_data: None,
            chain_status: self
                .app_status
                .map(models::NodeChainStatus::try_from)
                .transpose()?,
            sync_status: self
                .sync_status
                .map(models::NodeSyncStatus::try_from)
                .transpose()?,
            staking_status: self
                .staking_status
                .map(models::NodeStakingStatus::try_from)
                .transpose()?,
            container_status: self
                .container_status
                .map(models::ContainerStatus::try_from)
                .transpose()?,
            self_update: self.self_update,
        })
    }

    pub fn from_model(model: models::Node) -> Self {
        Self {
            id: model.id.to_string(),
            name: Some(model.name),
            ip: model.ip_addr,
            self_update: Some(model.self_update),
            block_height: model.block_height,
            onchain_name: None,
            app_status: Some(model.chain_status as i32),
            container_status: Some(model.container_status as i32),
            sync_status: Some(model.sync_status as i32),
            staking_status: model.staking_status.map(|ss| ss as i32),
            address: model.address,
            host_id: Some(model.host_id.to_string()),
        }
    }
}

#[tonic::async_trait]
impl Nodes for super::GrpcImpl {
    async fn info_update(
        &self,
        request: Request<blockjoy::NodeInfoUpdateRequest>,
    ) -> Result<Response<()>, Status> {
        self.db
            .trx(|c| {
                async move {
                    request
                        .into_inner()
                        .info
                        .ok_or_else(required("info"))?
                        .as_update()?
                        .update(c)
                        .await
                }
                .scope_boxed()
            })
            .await?;

        Ok(Response::new(()))
    }
}
