//! This module contains code regarding recovery from failed commands.

use displaydoc::Display;
use thiserror::Error;
use tonic::Status;
use tracing::error;

use crate::database::WriteConn;
use crate::grpc::api;
use crate::models::command::NewCommand;
use crate::models::node::{NewNodeLog, NodeLogEvent};
use crate::models::{Blockchain, Command, CommandType, IpAddress, Node};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Command recovery blockchain error: {0}
    Blockchain(#[from] crate::models::blockchain::Error),
    /// Failed to create cancelation log: {0}
    CancelationLog(crate::models::node::log::Error),
    /// Command error: {0}
    Command(#[from] crate::models::command::Error),
    /// CreateNode command has no node id.
    CreateNodeId,
    /// Failed to create deployment log: {0}
    DeploymentLog(crate::models::node::log::Error),
    /// Command recovery node error: {0}
    Node(#[from] crate::models::node::Error),
    /// Command recovery failed to update node: {0}
    UpdateNode(crate::models::node::Error),
    /// Unassigning an ip address failed: {0}
    UnassignIp(crate::models::ip_address::Error),
    /// Assigning an ip address failed: {0}
    AssignIp(crate::models::ip_address::Error),
    /// Finding an ip address failed: {0}
    FindIp(crate::models::ip_address::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            CreateNodeId => Status::invalid_argument("node_id"),
            Blockchain(err) => err.into(),
            CancelationLog(err) | DeploymentLog(err) => err.into(),
            Command(err) => err.into(),
            Node(err) | UpdateNode(err) => err.into(),
            UnassignIp(err) | AssignIp(err) | FindIp(err) => err.into(),
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
/// 1. Log that our creation has failed.
/// 2. Decide whether and where to re-create the node:
///    a. If this is the first failure on the current host, we try again with
///       the same host.
///    b. Otherwise, if this is the first host we tried on, we try again with a
///       new host.
///    c. Otherwise, we cannot recover.
/// 3. Use the previous decision to send a new create message to the right
///    instance of blockvisord, or mark the current node as failed and send an
///    MQTT message to the front end.
async fn recover_created(
    failed_cmd: &Command,
    write: &mut WriteConn<'_, '_>,
) -> Result<Vec<api::Command>, Error> {
    let mut vec = vec![];

    let node_id = failed_cmd.node_id.ok_or(Error::CreateNodeId)?;
    let mut node = Node::by_id(node_id, write).await?;
    let blockchain = Blockchain::by_id(node.blockchain_id, write).await?;

    // 1. We make a note in the node_logs table that creating our node failed. This may
    //    be unexpected, but we abort here when we fail to create that log. This is because the logs
    //    table is used to decide whether or not to retry. If logging our result failed, we may end
    //    up in an infinite loop.
    let new_log = NewNodeLog {
        host_id: node.host_id,
        node_id,
        event: NodeLogEvent::CreateFailed,
        blockchain_name: &blockchain.name,
        node_type: node.node_type,
        version: node.version.clone(),
        created_at: chrono::Utc::now(),
    };
    if let Err(err) = new_log.create(write).await {
        return Err(Error::DeploymentLog(err));
    };

    // We unassign the current ip address since we're going to be switching hosts.
    node.ip(write)
        .await?
        .unassign(write)
        .await
        .map_err(Error::UnassignIp)?;
    // 2. We now find the host that is next in line, and assign our node to that host.
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
            version: node.version,
            created_at: chrono::Utc::now(),
        };
        match new_log.create(write).await {
            Ok(_) => return Ok(vec![]),
            Err(err) => return Err(Error::CancelationLog(err)),
        }
    };
    let ip = IpAddress::next_for_host(host.id, write)
        .await
        .map_err(Error::FindIp)?;
    node.ip_addr = ip.ip().to_string();
    node.ip_gateway = host.ip_gateway.ip().to_string();
    node.host_id = host.id;
    let node = node.update(write).await.map_err(Error::UpdateNode)?;
    ip.assign(write).await.map_err(Error::AssignIp)?;

    // 3. We notify blockvisor of our retry via an MQTT message.
    if let Ok(cmd) = NewCommand::node(&node, CommandType::CreateNode)?
        .create(write)
        .await
    {
        let result = api::Command::from_model(&cmd, write).await;
        result.map_or_else(|_| {
            error!("Could not convert node create command to gRPC repr while recovering. Command: {:?}", cmd);
        }, |command| vec.push(command));
    } else {
        error!("Could not create node create command while recovering");
    }

    // we also start the node.
    if let Ok(cmd) = NewCommand::node(&node, CommandType::RestartNode)?
        .create(write)
        .await
    {
        let result = api::Command::from_model(&cmd, write).await;
        result.map_or_else(|_| {
            error!("Could not convert node start command to gRPC repr while recovering. Command {:?}", cmd);
        }, |command| vec.push(command));
    } else {
        error!("Could not create node start command while recovering");
    }

    Ok(vec)
}
