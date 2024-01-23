//! This module contains code regarding registering successful commands.

use tracing::error;

use crate::auth::AuthZ;
use crate::database::WriteConn;
use crate::grpc::api;
use crate::models::blockchain::Blockchain;
use crate::models::command::{Command, CommandType, NewCommand};
use crate::models::node::{NewNodeLog, Node, NodeLogEvent, NodeStatus, UpdateNode};

type Result = std::result::Result<(), ()>;

/// Some endpoints require some additional action from us when we recieve a
/// success message back from blockvisord.
///
/// For now this is limited to creating a `node_logs` entry when `CreateNode`
/// has succeeded, but this may expand over time.
pub(super) async fn register(
    succeeded_cmd: &Command,
    authz: &AuthZ,
    write: &mut WriteConn<'_, '_>,
) {
    let _ = match succeeded_cmd.command_type {
        CommandType::NodeCreate => create_node_success(succeeded_cmd, authz, write).await,
        CommandType::NodeDelete => delete_node_success(succeeded_cmd, write).await,
        _ => return,
    };
}

/// In case of a successful node deployment, we are expected to write `node_logs` entry to the
/// database. The `event` we pass in is `Succeeded`. Afterwards, we will start the node.
async fn create_node_success(
    succeeded_cmd: &Command,
    authz: &AuthZ,
    write: &mut WriteConn<'_, '_>,
) -> Result {
    let node_id = succeeded_cmd
        .node_id
        .ok_or_else(|| error!("`CreateNode` command has no node id!"))?;
    let node = Node::by_id(node_id, write)
        .await
        .map_err(|err| error!("Could not get node for node_id {node_id}: {err}"))?;
    let blockchain = Blockchain::by_id(node.blockchain_id, authz, write)
        .await
        .map_err(|err| error!("Could not get blockchain for node {node_id}: {err}"))?;

    let new_log = NewNodeLog {
        host_id: node.host_id,
        node_id,
        event: NodeLogEvent::CreateSucceeded,
        blockchain_id: blockchain.id,
        node_type: node.node_type,
        version: node.version.clone(),
        created_at: chrono::Utc::now(),
        org_id: node.org_id,
    };
    let _ = new_log.create(write).await;

    let start_notif = NewCommand::node(&node, CommandType::NodeRestart)
        .map_err(|err| error!("Command error: {err}"))?
        .create(write)
        .await
        .map_err(|err| error!("Could not insert new command into database: {err}"))?;
    let start_cmd = api::Command::from_model(&start_notif, authz, write)
        .await
        .map_err(|err| error!("Could not serialize new command to gRPC message: {err}"))?;
    write.mqtt(start_cmd);
    Ok(())
}

async fn delete_node_success(succeeded_cmd: &Command, write: &mut WriteConn<'_, '_>) -> Result {
    let command_id = succeeded_cmd.id;
    let node = succeeded_cmd
        .node(write)
        .await
        .map_err(|err| error!("Can't query node for command {command_id}: {err}!"))?
        .ok_or_else(|| error!("`DeleteNode` command {command_id} has no node!"))?;
    let node_id = node.id;
    let update = UpdateNode {
        node_status: Some(NodeStatus::Deleted),
        ..Default::default()
    };
    let node = node
        .update(update, write)
        .await
        .map_err(|err| error!("Failed to delete node {node_id} for command {command_id}: {err}"))?;
    Node::delete(node.id, write)
        .await
        .map_err(|err| error!("Failed to delete node {node_id} for command {command_id}: {err}"))?;
    Ok(())
}
