use diesel_async::scoped_futures::ScopedFutureExt;
use tracing::log::{debug, info};

use crate::auth::endpoint::Endpoint;
use crate::database::{Conn, Transaction, WriteConn};
use crate::models::command::NewCommand;
use crate::models::{Blockchain, CommandType, Node, NodeType};

use super::api::{self, babel_service_server};
use super::helpers::required;

// Implement the Babel service
#[tonic::async_trait]
impl babel_service_server::BabelService for super::Grpc {
    // Define the implementation of the upgrade method
    async fn notify(
        &self,
        req: tonic::Request<api::BabelServiceNotifyRequest>,
    ) -> super::Resp<api::BabelServiceNotifyResponse> {
        self.write(|write| notify(req, write).scope_boxed()).await
    }
}

async fn notify(
    req: tonic::Request<api::BabelServiceNotifyRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::BabelServiceNotifyResponse> {
    let WriteConn { conn, ctx, mqtt_tx } = write;

    // TODO: decide who is allowed to call this endpoint
    let _claims = ctx.claims(&req, Endpoint::BabelNotify, conn).await?;
    let req = req.into_inner();
    debug!("New Request Version: {:?}", req);
    let filter = req.info_filter(conn).await?;
    let nodes_to_upgrade = Node::outdated(&filter, conn).await?;
    debug!("Nodes to upgrade: {nodes_to_upgrade:?}");

    let blockchain = Blockchain::find_by_id(filter.blockchain_id, conn).await?;
    blockchain.add_version(&filter, conn).await?;
    let mut node_ids = vec![];
    for mut node in nodes_to_upgrade {
        let node_id = node.id.to_string();
        node.version = filter.version.clone();
        node.node_type = filter.node_type;
        let new_command = NewCommand {
            host_id: node.host_id,
            cmd: CommandType::UpgradeNode,
            sub_cmd: None,
            node_id: Some(node.id),
        };
        let node_updated = node.update(conn).await?;
        debug!("Node updated: {:?}", node_updated);
        let cmd = new_command.create(conn).await?;
        let command = api::Command::from_model(&cmd, conn).await?;
        debug!("Command sent: {:?}", command);
        mqtt_tx.send(command.into()).expect("mqtt_rx");
        node_ids.push(node_id);
    }

    info!("Nodes to be upgraded has been processed: {node_ids:?}");
    let resp = api::BabelServiceNotifyResponse { node_ids };

    Ok(tonic::Response::new(resp))
}

// impl api::BabelServiceNotifyRequest {
//     async fn info_filter(self, conn: &mut Conn<'_>) -> crate::Result<NodeSelfUpgradeFilter> {
//         let conf = self.config.ok_or_else(required("config"))?;
//         let node_type: NodeType = conf.node_type.parse().map_err(|e| {
//             crate::Error::BabelConfigConvertError(format!("Cannot convert node_type {e}"))
//         })?;
//         let blockchain = Blockchain::find_by_name(&conf.protocol, conn).await?;
//         Ok(NodeSelfUpgradeFilter {
//             version: conf.node_version,
//             node_type,
//             blockchain_id: blockchain.id,
//         })
//     }
// }
