use super::api::{self, babel_service_server};
use crate::auth;
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use tracing::log::{debug, info};

// Implement the Babel service
#[tonic::async_trait]
impl babel_service_server::BabelService for super::GrpcImpl {
    // Define the implementation of the upgrade method
    async fn notify(
        &self,
        req: tonic::Request<api::BabelServiceNotifyRequest>,
    ) -> super::Resp<api::BabelServiceNotifyResponse> {
        self.trx(|c| notify(self, req, c).scope_boxed()).await
    }
}

async fn notify(
    grpc: &super::GrpcImpl,
    req: tonic::Request<api::BabelServiceNotifyRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::BabelServiceNotifyResponse> {
    // TODO: decide who is allowed to call this endpoint
    let _claims = auth::get_claims(&req, auth::Endpoint::BabelNotifiy, conn).await?;
    let req = req.into_inner();
    debug!("New Request Version: {:?}", req);
    let filter = req.clone().try_into()?;
    let nodes_to_upgrade = models::Node::find_all_to_upgrade(&filter, conn).await?;
    debug!("Nodes to upgrade: {nodes_to_upgrade:?}",);

    let mut blockchain = models::Blockchain::find_by_name(&filter.blockchain, conn).await?;
    blockchain.set_new_supported_node_type_version(&filter)?;
    blockchain.update(conn).await?;
    debug!("Blockchain updated with new supported types: {blockchain:?}",);
    let mut node_ids = vec![];
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
        let node_updated = node.update(conn).await?;
        debug!("Node updated: {:?}", node_updated);
        let cmd = new_command.create(conn).await?;
        let command = api::Command::from_model(&cmd, conn).await?;
        grpc.notifier.commands_sender().send(&command).await?;
        debug!("Command sent: {:?}", command);
        node_ids.push(node_id);
    }

    info!("Nodes to be upgraded has been processed: {node_ids:?}",);
    let resp = api::BabelServiceNotifyResponse { node_ids };
    Ok(tonic::Response::new(resp))
}

impl TryFrom<api::BabelServiceNotifyRequest> for models::NodeSelfUpgradeFilter {
    type Error = crate::Error;

    fn try_from(req: api::BabelServiceNotifyRequest) -> crate::Result<Self> {
        req.config
            .map(|conf| {
                let node_type: models::NodeType = conf.node_type.parse().map_err(|e| {
                    crate::Error::BabelConfigConvertError(format!("Cannot convert node_type {e:?}"))
                })?;
                Ok(models::NodeSelfUpgradeFilter {
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
