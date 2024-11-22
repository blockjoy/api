use displaydoc::Display;
use thiserror::Error;
use tracing::warn;

use crate::auth::resource::NodeId;
use crate::auth::AuthZ;
use crate::database::WriteConn;
use crate::grpc::{api, Status};
use crate::model::blockchain::Blockchain;
use crate::model::command::{Command, CommandId, CommandType, NewCommand};
use crate::model::node::{
    NewNodeLog, Node, NodeLogEvent, NodeStatus, UpdateNode, UpdateNodeMetrics,
};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Command success blockchain error: {0}
    Blockchain(#[from] crate::model::blockchain::Error),
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
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Json(_) => Status::internal("Internal error."),
            MissingNodeId(_) => Status::invalid_argument("node_id"),
            DeleteNode(_, _, err) => err.into(),
            MqttStart(err) => (*err).into(),
            Blockchain(err) => err.into(),
            Command(err) => err.into(),
            Node(err) => err.into(),
        }
    }
}

/// Take additional action after receiving `ExitCode::Ok` from blockvisor.
pub(super) async fn register(
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
    let blockchain = Blockchain::by_id(node.blockchain_id, authz, write).await?;

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

    let start_cmd = NewCommand::node(&node, CommandType::NodeStart)?
        .create(write)
        .await?;
    let start_api = api::Command::from_model(&start_cmd, authz, write)
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
        block_height: None,
        block_age: None,
        staking_status: None,
        consensus: None,
        node_status: None,
        sync_status: None,
        jobs: Some(serde_json::from_str("[]").map_err(Error::Json)?),
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

    if node.deleted_at.is_none() {
        // TODO: This should go on a queue for inconsistencies that we register
        warn!("Received a deleted confirmation for a node that is not deleted");
    }

    let update = UpdateNode {
        node_status: Some(NodeStatus::Deleted),
        ..Default::default()
    };
    node.update(&update, write).await?;

    Ok(())
}
