//! This module contains code regarding registering successful commands.

use tracing::error;

use crate::database::WriteConn;
use crate::models::blockchain::Blockchain;
use crate::models::command::{Command, CommandType};
use crate::models::node::{NewNodeLog, Node, NodeLogEvent, NodeStatus};

type Result = std::result::Result<(), ()>;

/// Some endpoints require some additional action from us when we recieve a
/// success message back from blockvisord.
///
/// For now this is limited to creating a `node_logs` entry when `CreateNode`
/// has succeeded, but this may expand over time.
pub(super) async fn register(succeeded_cmd: &Command, write: &mut WriteConn<'_, '_>) {
    let _ = match succeeded_cmd.cmd {
        CommandType::CreateNode => create_node_success(succeeded_cmd, write).await,
        CommandType::DeleteNode => delete_node_success(succeeded_cmd, write).await,
        _ => return,
    };
}

/// In case of a successful node deployment, we are expected to write
/// `node_logs` entry to the database. The `event` we pass in is `Succeeded`.
async fn create_node_success(succeeded_cmd: &Command, write: &mut WriteConn<'_, '_>) -> Result {
    let node_id = succeeded_cmd
        .node_id
        .ok_or_else(|| error!("`CreateNode` command has no node id!"))?;
    let node = Node::find_by_id(node_id, write)
        .await
        .map_err(|err| error!("Could not get node for node_id {node_id}: {err}"))?;
    let blockchain = Blockchain::find_by_id(node.blockchain_id, write)
        .await
        .map_err(|err| error!("Could not get blockchain for node {node_id}: {err}"))?;

    let new_log = NewNodeLog {
        host_id: node.host_id,
        node_id,
        event: NodeLogEvent::CreateSucceeded,
        blockchain_name: &blockchain.name,
        node_type: node.node_type,
        version: node.version,
        created_at: chrono::Utc::now(),
    };
    let _ = new_log.create(write).await;
    Ok(())
}

async fn delete_node_success(succeeded_cmd: &Command, write: &mut WriteConn<'_, '_>) -> Result {
    let command_id = succeeded_cmd.id;
    let mut node = succeeded_cmd
        .node(write)
        .await
        .map_err(|err| error!("Can't query node for command {command_id}: {err}!"))?
        .ok_or_else(|| error!("`DeleteNode` command {command_id} has no node!"))?;
    let node_id = node.id;
    node.node_status = NodeStatus::Deleted;
    let node = node
        .update(write)
        .await
        .map_err(|err| error!("Failed to delete node {node_id} for command {command_id}: {err}"))?;
    Node::delete(node.id, write)
        .await
        .map_err(|err| error!("Failed to delete node {node_id} for command {command_id}: {err}"))?;
    Ok(())
}
