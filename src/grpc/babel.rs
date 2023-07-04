use diesel_async::scoped_futures::ScopedFutureExt;
use tracing::log::{debug, info};

use super::api::{self, babel_service_server};
use super::helpers::required;
use crate::auth::token::Endpoint;
use crate::{auth, models};

// Implement the Babel service
#[tonic::async_trait]
impl babel_service_server::BabelService for super::GrpcImpl {
    // Define the implementation of the upgrade method
    async fn notify(
        &self,
        req: tonic::Request<api::BabelServiceNotifyRequest>,
    ) -> super::Resp<api::BabelServiceNotifyResponse> {
        self.trx(|c| notify(req, c).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }
}

async fn notify(
    req: tonic::Request<api::BabelServiceNotifyRequest>,
    conn: &mut models::Conn,
) -> crate::Result<super::Outcome<api::BabelServiceNotifyResponse>> {
    // TODO: decide who is allowed to call this endpoint
    let _claims = auth::get_claims(&req, Endpoint::BabelNotifiy, conn).await?;
    let req = req.into_inner();
    debug!("New Request Version: {:?}", req);
    let filter = req.info_filter(conn).await?;
    let nodes_to_upgrade = models::Node::find_all_to_upgrade(&filter, conn).await?;
    debug!("Nodes to upgrade: {nodes_to_upgrade:?}",);

    let blockchain = models::Blockchain::find_by_id(filter.blockchain_id, conn).await?;
    blockchain.add_version(&filter, conn).await?;
    let mut node_ids = vec![];
    let mut commands = vec![];
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
        debug!("Command sent: {:?}", command);
        commands.push(command);
        node_ids.push(node_id);
    }

    info!("Nodes to be upgraded has been processed: {node_ids:?}",);
    let resp = api::BabelServiceNotifyResponse { node_ids };
    Ok(super::Outcome::new(resp).with_msgs(commands))
}

impl api::BabelServiceNotifyRequest {
    async fn info_filter(
        self,
        conn: &mut models::Conn,
    ) -> crate::Result<models::NodeSelfUpgradeFilter> {
        let conf = self.config.ok_or_else(required("config"))?;
        let node_type: models::NodeType = conf.node_type.parse().map_err(|e| {
            crate::Error::BabelConfigConvertError(format!("Cannot convert node_type {e}"))
        })?;
        let blockchain = models::Blockchain::find_by_name(&conf.protocol, conn).await?;
        Ok(models::NodeSelfUpgradeFilter {
            version: conf.node_version,
            node_type,
            blockchain_id: blockchain.id,
        })
    }
}
