use std::str::FromStr;

use crate::models::{self, NodeSelfUpgradeFilter, NodeType};
use diesel_async::scoped_futures::ScopedFutureExt;
use tokio::task::JoinSet;
use tonic::Request;

// Import generated proto code
use super::api::{babel_service_server::BabelService, BabelNewVersionRequest};

// Implement the Babel service
#[tonic::async_trait]
impl BabelService for super::GrpcImpl {
    // Define the implementation of the upgrade method
    async fn notify(&self, request: Request<BabelNewVersionRequest>) -> super::Result<()> {
        let refresh_token = super::get_refresh_token(&request);
        let req = request.into_inner();
        self.trx(|c| {
            async move {
                let filter = req
                    .clone()
                    .try_into()
                    .map_err(<crate::Error as Into<tonic::Status>>::into)?;
                let nodes_to_upgrade = models::Node::find_all_to_upgrade(&filter, c)
                    .await
                    .map_err(<crate::Error as Into<tonic::Status>>::into)?;
                let mut tasks = JoinSet::new();
                nodes_to_upgrade.into_iter().for_each(|node| {
                    let handler = self.clone();
                    let req_cloned = req.clone();
                    tasks.spawn(
                        async move { handler.send_upgrade_command(&node, req_cloned).await },
                    );
                });
                while let Some(task) = tasks.join_next().await {
                    if let Err(error) = task.unwrap() {
                        tracing::error!("Error upgrading nodes {error:?}");
                        return Err(crate::Error::UpgradeProcessError(format!(
                            "Error upgrading nodes {error:?}"
                        )));
                    }
                }
                Ok(())
            }
            .scope_boxed()
        })
        .await?;
        Ok(super::response_with_refresh_token(refresh_token, ())?)
    }
}

impl super::GrpcImpl {
    async fn send_upgrade_command(
        &self,
        node: &models::Node,
        request: BabelNewVersionRequest,
    ) -> crate::Result<()> {
        //let host_id = req.host_id(c).await?;
        //let node_id = req.node_id()?;
        //let command_type = req.command_type()?;
        //let command = req
        //    .as_new(host_id, node_id, command_type)?
        //    .create(c)
        //    .await?;
        //let command = api::Command::from_model(&command, c).await?;
        //self.notifier.commands_sender().send(&command).await?;
        //let response = api::CommandServiceCreateResponse {
        //    command: Some(command),
        //};
        Ok(())
    }
}

impl TryFrom<BabelNewVersionRequest> for models::NodeSelfUpgradeFilter {
    type Error = crate::Error;
    fn try_from(req: BabelNewVersionRequest) -> crate::Result<Self> {
        req.config
            .map(|conf| {
                let node_type = NodeType::from_str(&conf.node_type).map_err(|e| {
                    crate::Error::BabelConfigConvertError(
                        "Cannot convert node_type {e:?}".to_string(),
                    )
                })?;
                Ok(NodeSelfUpgradeFilter {
                    version: conf.node_version,
                    node_type,
                    blockchain: conf.protocol.into(),
                })
            })
            .unwrap_or_else(|| {
                Err(crate::Error::BabelConfigConvertError(
                    "No config provided".into(),
                ))
            })
    }
}
