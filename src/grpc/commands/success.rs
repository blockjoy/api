//! This module contains code regarding registering successful commands.

use crate::{auth::FindableById, models};
use diesel_async::AsyncPgConnection;

/// Some endpoints require some additional action from us when we recieve a success message back
/// from blockvisord. For now this is limited to creating a node_logs entry when
/// CreateNode has succeeded, but this may expand over time.
pub(super) async fn register(succeeded: &models::Command, conn: &mut AsyncPgConnection) {
    if succeeded.cmd == models::CommandType::CreateNode {
        create_node_success(succeeded, conn).await;
    }
}

/// In case of a successful node deployment, we are expected to write node_logs entry to
/// the database. The `event` we pass in is `Succeeded`.
async fn create_node_success(succeeded: &models::Command, conn: &mut AsyncPgConnection) {
    let Some(node_id) = succeeded.node_id else {
        tracing::error!("`CreateNode` command has no node id!");
        return;
    };
    let Ok(node) = models::Node::find_by_id(node_id, conn).await else {
        tracing::error!("Could not get node for node_id {node_id}");
        return;
    };
    let Ok(blockchain) = models::Blockchain::find_by_id(node.blockchain_id, conn).await else {
        tracing::error!("Could not get blockchain for node {node_id}");
        return;
    };

    let new_log = models::NewNodeLog {
        host_id: node.host_id,
        node_id,
        event: models::NodeLogEvent::Succeeded,
        blockchain_name: &blockchain.name,
        node_type: node.node_type,
        version: node.version.as_deref().unwrap_or("latest"),
        created_at: chrono::Utc::now(),
    };
    let _ = new_log.create(conn).await;
}
