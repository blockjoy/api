//! This module contains code regarding recovery from failed commands.

use displaydoc::Display;
use thiserror::Error;
use tracing::{error, warn};

use crate::auth::resource::{HostId, OrgId};
use crate::auth::AuthZ;
use crate::database::WriteConn;
use crate::grpc::{api, Status};
use crate::model::command::NewCommand;
use crate::model::node::{LogEvent, NewNodeLog, UpdateNode};
use crate::model::{Command, CommandType, Host, IpAddress, Node, Protocol};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create cancelled log: {0}
    CancelledLog(crate::model::node::log::Error),
    /// Command recovery error: {0}
    Command(#[from] crate::model::command::Error),
    /// Recovery of `CreateNode` command has no node id.
    CreateNodeId,
    /// Command recovery dns error: {0}
    Dns(#[from] crate::cloudflare::Error),
    /// Failed to create deployment log: {0}
    DeploymentLog(crate::model::node::log::Error),
    /// Command recovery host error: {0}
    Host(#[from] crate::model::host::Error),
    /// Command recovery ip address: {0}
    IpAddress(#[from] crate::model::ip_address::Error),
    /// Command recovery node error: {0}
    Node(#[from] crate::model::node::Error),
    /// No IP addresses available for host: {0}
    NoIpForHost(HostId),
    /// Command protocol error: {0}
    Protocol(#[from] crate::model::protocol::Error),
    /// Command recovery failed to update node: {0}
    UpdateNode(crate::model::node::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Dns(_) | NoIpForHost(_) => Status::internal("Internal error."),
            CreateNodeId => Status::invalid_argument("node_id"),
            CancelledLog(err) => err.into(),
            Command(err) => err.into(),
            DeploymentLog(err) => err.into(),
            Host(err) => err.into(),
            IpAddress(err) => err.into(),
            Node(err) | UpdateNode(err) => err.into(),
            Protocol(err) => err.into(),
        }
    }
}

/// Attempt to recover from a failed `Command`.
pub(super) async fn recover(
    failed: &Command,
    org_id: Option<OrgId>,
    authz: &AuthZ,
    write: &mut WriteConn<'_, '_>,
) -> Result<Vec<api::Command>, Error> {
    match failed.command_type {
        CommandType::NodeCreate => node_create_failed(failed, org_id, authz, write).await,
        _ => Ok(vec![]),
    }
}

/// Recover from a failed node creation.
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
async fn node_create_failed(
    failed: &Command,
    org_id: Option<OrgId>,
    authz: &AuthZ,
    write: &mut WriteConn<'_, '_>,
) -> Result<Vec<api::Command>, Error> {
    let mut vec = vec![];

    let node_id = failed.node_id.ok_or(Error::CreateNodeId)?;
    let node = Node::by_id(node_id, write).await?;
    Host::remove_node(&node, write).await?;

    // 1. We make a note in the node_logs table that creating our node failed.
    let _ = NewNodeLog::from(&node, authz, LogEvent::CreateFailed)
        .create(write)
        .await
        .map_err(Error::DeploymentLog)?;

    if let Err(err) = write.ctx.dns.delete(&node.dns_id).await {
        warn!("Failed to remove node dns for node {}: {err}", node.id);
    }

    // 2. We now find the next host to assign our node to.
    let protocol = Protocol::by_id(node.protocol_id, org_id, authz, write).await?;
    let Some(host) = node.next_host(&protocol, write).await? else {
        return NewNodeLog::from(&node, authz, LogEvent::CreateCancelled)
            .create(write)
            .await
            .map(|_log| vec![])
            .map_err(Error::CancelledLog);
    };

    let ip_address = IpAddress::next_for_host(host.id, write)
        .await?
        .ok_or_else(|| Error::NoIpForHost(host.id))?;

    let update = UpdateNode {
        org_id: None,
        host_id: Some(host.id),
        display_name: None,
        auto_upgrade: None,
        ip_address: Some(ip_address.ip),
        ip_gateway: Some(host.ip_gateway),
        note: None,
        tags: None,
    };
    let node = update
        .apply(node_id, authz, write)
        .await
        .map_err(Error::UpdateNode)?;

    Host::add_node(&node, write).await?;

    write
        .ctx
        .dns
        .create(&node.dns_name, ip_address.ip.ip())
        .await?;

    // 3. We notify blockvisor of our retry via an MQTT message.
    if let Ok(cmd) = NewCommand::node(&node, CommandType::NodeCreate)?
        .create(write)
        .await
    {
        let result = api::Command::from(&cmd, authz, write).await;
        result.map_or_else(|_| {
            error!("Could not convert node create command to gRPC repr while recovering. Command: {:?}", cmd);
        }, |command| vec.push(command));
    } else {
        error!("Could not create node create command while recovering");
    }

    // we also start the node.
    if let Ok(cmd) = NewCommand::node(&node, CommandType::NodeRestart)?
        .create(write)
        .await
    {
        let result = api::Command::from(&cmd, authz, write).await;
        result.map_or_else(|_| {
            error!("Could not convert node start command to gRPC repr while recovering. Command {:?}", cmd);
        }, |command| vec.push(command));
    } else {
        error!("Could not create node start command while recovering");
    }

    Ok(vec)
}
