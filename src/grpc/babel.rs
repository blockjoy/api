use std::str::FromStr;

use crate::models::{self, NodeSelfUpgradeFilter, NodeType};
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::Request;
use tracing::log::{debug, info};

use super::api::{self, BabelNewVersionResponse};
// Import generated proto code
use super::api::{babel_service_server::BabelService, BabelNewVersionRequest};

// Implement the Babel service
#[tonic::async_trait]
impl BabelService for super::GrpcImpl {
    // Define the implementation of the upgrade method
    async fn notify(
        &self,
        request: Request<BabelNewVersionRequest>,
    ) -> super::Result<BabelNewVersionResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let req = request.into_inner();
        debug!("New Request Version: {:?}", req);
        let mut conn = self.conn().await?;
        let filter = req
            .clone()
            .try_into()
            .map_err(<crate::Error as Into<tonic::Status>>::into)?;
        let nodes_to_upgrade = models::Node::find_all_to_upgrade(&filter, &mut conn)
            .await
            .map_err(<crate::Error as Into<tonic::Status>>::into)?;
        debug!("Nodes to upgrade: {:?}", nodes_to_upgrade);

        let upgraded_nodes = self
            .trx(|c| {
                async move {
                    let mut blockchain =
                        models::Blockchain::find_by_name(&filter.blockchain, c).await?;
                    blockchain.set_new_supported_node_type_version(&filter)?;
                    blockchain.update(c).await?;
                    debug!(
                        "Blockchain updated with new supported types: {:?}",
                        blockchain
                    );
                    let mut nodes_ids = vec![];
                    for mut node in nodes_to_upgrade {
                        let node_id = node.id.to_string();
                        node.version = filter.version.clone();
                        node.node_type = filter.node_type;
                        let new_command = models::NewCommand {
                            host_id: node.host_id,
                            cmd: models::CommandType::UpgradeNode,
                            sub_cmd: None,
                            node_id: Some(node.id),
                        };
                        let node_updated = node.update(c).await?;
                        debug!("Node updated: {:?}", node_updated);
                        let cmd = new_command.create(c).await?;
                        let command = api::Command::from_model(&cmd, c).await?;
                        self.notifier.commands_sender().send(&command).await?;
                        debug!("Command sent: {:?}", command);
                        nodes_ids.push(node_id);
                    }
                    Ok(nodes_ids)
                }
                .scope_boxed()
            })
            .await?;

        info!(
            "Nodes to be upgraded has been processed: {:?}",
            upgraded_nodes
        );
        let response = BabelNewVersionResponse {
            nodes_ids: upgraded_nodes,
        };
        Ok(super::response_with_refresh_token(refresh_token, response)?)
    }
}

impl TryFrom<BabelNewVersionRequest> for models::NodeSelfUpgradeFilter {
    type Error = crate::Error;
    fn try_from(req: BabelNewVersionRequest) -> crate::Result<Self> {
        req.config
            .map(|conf| {
                let node_type = NodeType::from_str(&conf.node_type).map_err(|e| {
                    crate::Error::BabelConfigConvertError(format!("Cannot convert node_type {e:?}"))
                })?;
                Ok(NodeSelfUpgradeFilter {
                    version: conf.node_version,
                    node_type,
                    blockchain: conf.protocol,
                })
            })
            .unwrap_or_else(|| {
                Err(crate::Error::BabelConfigConvertError(
                    "No config provided".into(),
                ))
            })
    }
}
