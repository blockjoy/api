//! This module contains code regarding recovery from failed commands.

use std::vec;

use tracing::error;

use crate::grpc::{self, api};
use crate::models;

/// When we get a failed command back from blockvisord, we can try to recover from this. This is
/// currently only implemented for failed node creates. Note that this function largely ignores
/// errors. We are already in a state where we are trying to recover from a failure mode, so we will
/// make our best effort to recover. If a command won't send but it not essential for process, we
/// ignore and continue.
pub(super) async fn recover(
    failed_cmd: &models::Command,
    conn: &mut models::Conn,
) -> crate::Result<Vec<api::Command>> {
    if failed_cmd.cmd == models::CommandType::CreateNode {
        recover_created(failed_cmd, conn).await
    } else {
        Ok(vec![])
    }
}

async fn recover_created(
    failed_cmd: &models::Command,
    conn: &mut models::Conn,
) -> crate::Result<Vec<api::Command>> {
    let mut vec = vec![];
    let Some(node_id) = failed_cmd.node_id else {
        error!("`CreateNode` command has no node id!");
        return Err(crate::Error::ValidationError (
            "CreateNode command has no node id".to_string(),
        ));
    };
    // Recovery from a failed delete looks like this:
    // 1. Send a message to blockvisord to delete the old node.
    // 2. Log that our creation has failed.
    // 3. Decide whether and where to re-create the node:
    //    a. If this is the first failure on the current host, we try again with the same host.
    //    b. Otherwise, if this is the first host we tried on, we try again with a new host.
    //    c. Otherwise, we cannot recover.
    // 4. Use the previous decision to send a new create message to the right instance of
    //    blockvisord, or mark the current node as failed and send an MQTT message to the front end.
    let Ok(mut node) = models::Node::find_by_id(node_id, conn).await else {
        error!("Could not get node for node_id {node_id}");
        return Err(crate::Error::ValidationError (
            "Could not get node for node_id".to_string(),
        ));
    };
    let Ok(blockchain) = models::Blockchain::find_by_id(node.blockchain_id, conn).await else {
        error!("Could not get blockchain for node {node_id}");
        return Err(crate::Error::ValidationError (
            "Could not get blockchain for node".to_string(),
        ));
    };

    // 1. We send a delete to blockvisord to help it with cleanup.
    send_delete(&node, &mut vec, conn).await;

    // 2. We make a note in the node_logs table that creating our node failed. This may
    //    be unexpected, but we abort here when we fail to create that log. This is because the logs
    //    table is used to decide whether or not to retry. If logging our result failed, we may end
    //    up in an infinite loop.
    let new_log = models::NewNodeLog {
        host_id: node.host_id,
        node_id,
        event: models::NodeLogEvent::Failed,
        blockchain_name: &blockchain.name,
        node_type: node.node_type,
        version: &node.version,
        created_at: chrono::Utc::now(),
    };
    if let Err(e) = new_log.create(conn).await {
        let msg = format!("Failed to create deployment log entry: {e}");
        error!("{msg}");
        return Err(crate::Error::ValidationError(msg));
    };

    // 3. We now find the host that is next in line, and assign our node to that host.
    let Ok(host) = node.find_host(conn).await else {
        // We were unable to find a new host. This may happen because the system is out of resources
        // or because we have retried to many times. Either way we have to log that this retry was
        // canceled.
        let new_log = models::NewNodeLog {
            host_id: node.host_id,
            node_id,
            event: models::NodeLogEvent::Canceled,
            blockchain_name: &blockchain.name,
            node_type: node.node_type,
            version: &node.version,
            created_at: chrono::Utc::now(),
        };
        match new_log.create(conn).await {
            Ok(_) => return Ok(vec![]),
            Err(e) => {
                let msg = format!("Failed to create cancelation log entry: {e}");
                error!(msg);
                return Err(crate::Error::ValidationError(msg));
            }
        }
    };
    node.host_id = host.id;
    let node = match node.update(conn).await {
        Ok(node) => node,
        Err(e) => {
            let msg = format!("Could not update node: {e}");
            error!("{msg}");
            return Err(crate::Error::ValidationError(msg));
        }
    };

    // 4. We notify blockvisor of our retry via an MQTT message.
    if let Ok(cmd) =
        grpc::nodes::create_node_command(&node, models::CommandType::CreateNode, conn).await
    {
        if let Ok(create_cmd) = api::Command::from_model(&cmd, conn).await {
            vec.push(create_cmd)
        } else {
            error!("Could not convert node create command to gRPC repr while recovering. Command: {:?}", cmd);
        }
    } else {
        error!("Could not create node create command while recovering");
    }
    // we also start the node.
    if let Ok(cmd) =
        grpc::nodes::create_node_command(&node, models::CommandType::RestartNode, conn).await
    {
        if let Ok(start_cmd) = api::Command::from_model(&cmd, conn).await {
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
async fn send_delete(
    node: &models::Node,
    commands: &mut Vec<api::Command>,
    conn: &mut models::Conn,
) {
    let node_id = node.id.to_string();
    let cmd = models::NewCommand {
        host_id: node.host_id,
        cmd: models::CommandType::DeleteNode,
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
