// Import necessary libraries
use futures::stream::StreamExt;
use tokio::sync::mpsc;
use tonic::{Request, Response, Status};

use crate::models::{self, NodeSelfUpgradeFilter};

// Import generated proto code
use super::api::{
    babel_service_server::BabelService, BabelNewVersionRequest, BabelNewVersionResponse, Node,
};

// Implement the Babel service
#[tonic::async_trait]
impl BabelService for super::GrpcImpl {
    // Define the implementation of the upgrade method
    async fn notify(
        &self,
        request: Request<BabelNewVersionRequest>,
    ) -> super::Result<BabelNewVersionResponse> {
        let refresh_token = super::get_refresh_token(&req);
        let req = request.into_inner();
        self.trx(|c| {
            async move {
                let filter = req.try_into()?;
                let host_id = req.host_id(c).await?;
                let node_id = req.node_id()?;
                let command_type = req.command_type()?;
                let command = req
                    .as_new(host_id, node_id, command_type)?
                    .create(c)
                    .await?;
                let command = api::Command::from_model(&command, c).await?;
                self.notifier.commands_sender().send(&command).await?;
                let response = api::CommandServiceCreateResponse {
                    command: Some(command),
                };

                Ok(super::response_with_refresh_token(refresh_token, response)?)
            }
            .scope_boxed()
        })
        .await
    }
}

impl TryFrom<BabelNewVersionRequest> for models::NodeSelfUpgradeFilter {
    type Error = crate::Error;
    fn try_from(req: BabelNewVersionRequest) -> crate::Result<Self> {
        req.config
            .map(|conf| NodeSelfUpgradeFilter {
                version: conf.node_version,
                node_type: conf.node_type.into(),
                blockchain: conf.protocol.into(),
            })
            .ok_or_else(|| crate::Error::BabelConfigConvertError("No config provided".into()))
    }
}
