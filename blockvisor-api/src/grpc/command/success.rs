use displaydoc::Display;
use thiserror::Error;

use crate::auth::resource::NodeId;
use crate::auth::AuthZ;
use crate::database::WriteConn;
use crate::grpc::api;
use crate::model::command::{Command, CommandType, NewCommand};
use crate::model::node::{
    LogEvent, NewNodeLog, Node, NodeJobs, NodeState, UpdateNodeMetrics, UpdateNodeState,
};
use crate::model::CommandId;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Command success model error: {0}
    Command(#[from] crate::model::command::Error),
    /// Command `{0}` failedto delete node `{1}`: {2}
    DeleteNode(CommandId, NodeId, crate::model::node::Error),
    /// Failed to serialize JSON: {0}
    Json(serde_json::Error),
    /// Command `{0}` is missing the `NodeId`.
    MissingNodeId(CommandId),
    /// Failed to write a NodeStart command to MQTT: {0}
    MqttStart(Box<super::Error>),
    /// Command success node error: {0}
    Node(#[from] crate::model::node::Error),
    /// Command success node log error: {0}
    NodeLog(#[from] crate::model::node::log::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Json(_) => Status::internal("Internal error."),
            MissingNodeId(_) => Status::invalid_argument("node_id"),
            DeleteNode(_, _, err) => err.into(),
            MqttStart(err) => (*err).into(),
            Command(err) => err.into(),
            Node(err) => err.into(),
            NodeLog(err) => err.into(),
        }
    }
}

/// Confirm success and take additional action after receiving `ExitCode::Ok`.
pub(super) async fn confirm(
    cmd: &Command,
    authz: &AuthZ,
    write: &mut WriteConn<'_, '_>,
) -> Result<(), Error> {
    match cmd.command_type {
        CommandType::NodeCreate => node_created(cmd, authz, write).await,
        CommandType::NodeUpgrade => node_upgraded(cmd, write).await,
        CommandType::NodeDelete => node_deleted(cmd, write).await,
        _ => Ok(()),
    }
}

/// After NodeCreate, write a log and send a start command.
async fn node_created(
    cmd: &Command,
    authz: &AuthZ,
    write: &mut WriteConn<'_, '_>,
) -> Result<(), Error> {
    let node_id = cmd.node_id.ok_or_else(|| Error::MissingNodeId(cmd.id))?;
    let node = Node::by_id(node_id, write).await?;

    NewNodeLog::from(&node, authz, LogEvent::CreateSucceeded)
        .create(write)
        .await?;

    let start_cmd = NewCommand::node(&node, CommandType::NodeStart)?
        .create(write)
        .await?;
    let start_api = api::Command::from(&start_cmd, Some(node.org_id), authz, write)
        .await
        .map_err(|err| Error::MqttStart(Box::new(err)))?;
    write.mqtt(start_api);

    Ok(())
}

/// After NodeUpgrade, clear out any old jobs.
async fn node_upgraded(cmd: &Command, write: &mut WriteConn<'_, '_>) -> Result<(), Error> {
    let node_id = cmd.node_id.ok_or_else(|| Error::MissingNodeId(cmd.id))?;
    let update = UpdateNodeMetrics {
        id: node_id,
        node_state: None,
        protocol_state: None,
        protocol_health: None,
        block_height: None,
        block_age: None,
        consensus: None,
        jobs: Some(NodeJobs(vec![])),
    };
    let _updated = update.apply(write).await?;

    Ok(())
}

/// After NodeDelete, set the node status to deleted.
async fn node_deleted(cmd: &Command, write: &mut WriteConn<'_, '_>) -> Result<(), Error> {
    let node = cmd
        .node(write)
        .await?
        .ok_or_else(|| Error::MissingNodeId(cmd.id))?;

    let update = UpdateNodeState {
        id: node.id,
        node_state: Some(NodeState::Deleted),
        next_state: Some(None),
        protocol_state: None,
        protocol_health: None,
        p2p_address: None,
    };
    let updated = update.apply(write).await?;

    Node::delete(updated.id, write).await.map_err(Into::into)
}
