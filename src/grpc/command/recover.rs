//! This module contains code regarding recovery from failed commands.

use displaydoc::Display;
use thiserror::Error;
use tonic::Status;
use tracing::error;

use crate::database::{Conn, WriteConn};
use crate::grpc::{api, node};
use crate::models::command::NewCommand;
use crate::models::node::{NewNodeLog, NodeLogEvent};
use crate::models::{Blockchain, Command, CommandType, Node};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Command recovery blockchain error: {0}
    Blockchain(#[from] crate::models::blockchain::Error),
    /// Failed to create cancelation log: {0}
    CancelationLog(crate::models::node::log::Error),
    /// CreateNode command has no node id.
    CreateNodeId,
    /// Failed to create deployment log: {0}
    DeploymentLog(crate::models::node::log::Error),
    /// Command recovery node error: {0}
    Node(#[from] crate::models::node::Error),
    /// Command recovery failed to update node: {0}
    UpdateNode(crate::models::node::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!("{err}");
        use Error::*;
        match err {
            CreateNodeId => Status::invalid_argument("node_id"),
            Blockchain(err) => err.into(),
            CancelationLog(err) | DeploymentLog(err) => err.into(),
            Node(err) | UpdateNode(err) => err.into(),
        }
    }
}

/// When we get a failed command back from blockvisord, we can try to recover from this. This is
/// currently only implemented for failed node creates. Note that this function largely ignores
/// errors. We are already in a state where we are trying to recover from a failure mode, so we will
/// make our best effort to recover. If a command won't send but it not essential for process, we
/// ignore and continue.
pub(super) async fn recover(
    failed_cmd: &Command,
    write: &mut WriteConn<'_, '_>,
) -> Result<Vec<api::Command>, Error> {
    if failed_cmd.cmd == CommandType::CreateNode {
        recover_created(failed_cmd, write).await
    } else {
        Ok(vec![])
    }
}

/// Recover from a failed delete.
///
/// 1. Send a message to blockvisord to delete the old node.
/// 2. Log that our creation has failed.
/// 3. Decide whether and where to re-create the node:
///    a. If this is the first failure on the current host, we try again with
///       the same host.
///    b. Otherwise, if this is the first host we tried on, we try again with a
///       new host.
///    c. Otherwise, we cannot recover.
/// 4. Use the previous decision to send a new create message to the right
///    instance of blockvisord, or mark the current node as failed and send an
///    MQTT message to the front end.
async fn recover_created(
    failed_cmd: &Command,
    write: &mut WriteConn<'_, '_>,
) -> Result<Vec<api::Command>, Error> {
    let mut vec = vec![];

    let node_id = failed_cmd.node_id.ok_or(Error::CreateNodeId)?;
    let mut node = Node::find_by_id(node_id, write).await?;
    let blockchain = Blockchain::find_by_id(node.blockchain_id, write).await?;

    // 1. We send a delete to blockvisord to help it with cleanup.
    send_delete(&node, &mut vec, write).await;

    // 2. We make a note in the node_logs table that creating our node failed. This may
    //    be unexpected, but we abort here when we fail to create that log. This is because the logs
    //    table is used to decide whether or not to retry. If logging our result failed, we may end
    //    up in an infinite loop.
    let new_log = NewNodeLog {
        host_id: node.host_id,
        node_id,
        event: NodeLogEvent::Failed,
        blockchain_name: &blockchain.name,
        node_type: node.node_type,
        version: &node.version,
        created_at: chrono::Utc::now(),
    };
    if let Err(err) = new_log.create(write).await {
        return Err(Error::DeploymentLog(err));
    };

    // 3. We now find the host that is next in line, and assign our node to that host.
    let Ok(host) = node.find_host(write).await else {
        // We were unable to find a new host. This may happen because the system is out of resources
        // or because we have retried to many times. Either way we have to log that this retry was
        // canceled.
        let new_log = NewNodeLog {
            host_id: node.host_id,
            node_id,
            event: NodeLogEvent::Canceled,
            blockchain_name: &blockchain.name,
            node_type: node.node_type,
            version: &node.version,
            created_at: chrono::Utc::now(),
        };
        match new_log.create(write).await {
            Ok(_) => return Ok(vec![]),
            Err(err) => return Err(Error::CancelationLog(err)),
        }
    };
    node.host_id = host.id;
    let node = node.update(write).await.map_err(Error::UpdateNode)?;

    // 4. We notify blockvisor of our retry via an MQTT message.
    if let Ok(cmd) = node::create_node_command(&node, CommandType::CreateNode, write).await {
        if let Ok(create_cmd) = api::Command::from_model(&cmd, write).await {
            vec.push(create_cmd)
        } else {
            error!("Could not convert node create command to gRPC repr while recovering. Command: {:?}", cmd);
        }
    } else {
        error!("Could not create node create command while recovering");
    }
    // we also start the node.
    if let Ok(cmd) = node::create_node_command(&node, CommandType::RestartNode, write).await {
        if let Ok(start_cmd) = api::Command::from_model(&cmd, write).await {
            vec.push(start_cmd);
        } else {
            error!(
                "Could not convert node start command to gRPC repr while recovering. Command {:?}",
                cmd
            );
        }
    } else {
        error!("Could not create node start command while recovering");
    }
    Ok(vec)
}

/// Send a delete message to blockvisord, to delete the given node. We do this to assist blockvisord
/// to clean up after a failed node create.
async fn send_delete(node: &Node, commands: &mut Vec<api::Command>, conn: &mut Conn<'_>) {
    let node_id = node.id.to_string();
    let cmd = NewCommand {
        host_id: node.host_id,
        cmd: CommandType::DeleteNode,
        // NOTE: the node id goes into the sub_cmd field, since the node has just been deleted, so
        // using the `node_id` field would cause an integrity error.
        sub_cmd: Some(&node_id),
        node_id: None,
    };
    let Ok(cmd) = cmd.create(conn).await else {
        error!("Could not create node delete command while recovering");
        return;
    };
    let Ok(cmd) = api::Command::from_model(&cmd, conn).await else {
        error!("Could not convert node delete command to gRPC repr while recovering");
        return;
    };
    commands.push(cmd);
}
