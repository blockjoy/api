mod recover;
mod success;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::{error, warn};

use crate::auth::rbac::{CommandAdminPerm, CommandPerm};
use crate::auth::resource::{OrgId, Resource, Resources};
use crate::auth::{AuthZ, Authorize};
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::grpc::api::command_service_server::CommandService;
use crate::grpc::{api, common, Grpc};
use crate::model::command::{CommandFilter, ExitCode, UpdateCommand};
use crate::model::node::{NextState, NodeState, UpdateNodeState};
use crate::model::{Command, CommandType, Host, Node};
use crate::util::NanosUtc;

use super::Metadata;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Command model failure: {0}
    Command(#[from] crate::model::command::Error),
    /// Command image config failure: {0}
    Config(#[from] crate::model::image::config::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Error creating a gRPC representation of a node: {0}
    GrpcHost(Box<crate::grpc::node::Error>),
    /// Command host error: {0}
    Host(#[from] crate::model::host::Error),
    /// List commands is missing a node_id or host_id.
    ListMissingNodeOrHost,
    /// Missing `command.node_id`.
    MissingNodeId,
    /// Command node error: {0}
    Node(#[from] crate::model::node::Error),
    /// Command node response error: {0}
    NodeResponse(Box<crate::grpc::node::Error>),
    /// Failed to parse HostId: {0}
    ParseHostId(uuid::Error),
    /// Failed to parse NodeId: {0}
    ParseNodeId(uuid::Error),
    /// Failed to parse CommandId: {0}
    ParseId(uuid::Error),
    /// Command protocol error: {0}
    Protocol(#[from] crate::model::protocol::Error),
    /// Command protocol version error: {0}
    ProtocolVersion(#[from] crate::model::protocol::version::Error),
    /// Failed to recover from a failed command: {0}
    Recover(#[from] self::recover::Error),
    /// Unable to cast retry hint from u64 to i64: {0}
    RetryHint(std::num::TryFromIntError),
    /// Resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// Command success error: {0}
    Success(#[from] self::success::Error),
    /// Unknown ExitCode.
    UnknownExitCode,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | GrpcHost(_) => Status::internal("Internal error."),
            ListMissingNodeOrHost => Status::invalid_argument("node_id or host_id"),
            MissingNodeId => Status::invalid_argument("command.node_id"),
            ParseNodeId(_) => Status::invalid_argument("node_id"),
            ParseHostId(_) => Status::invalid_argument("host_id"),
            ParseId(_) => Status::invalid_argument("id"),
            RetryHint(_) => Status::invalid_argument("retry_hint_seconds"),
            UnknownExitCode => Status::invalid_argument("exit_code"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Command(err) => err.into(),
            Config(err) => err.into(),
            Host(err) => err.into(),
            Node(err) => err.into(),
            NodeResponse(err) => (*err).into(),
            Protocol(err) => err.into(),
            ProtocolVersion(err) => err.into(),
            Recover(err) => err.into(),
            Resource(err) => err.into(),
            Success(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl CommandService for Grpc {
    async fn ack(
        &self,
        req: Request<api::CommandServiceAckRequest>,
    ) -> Result<Response<api::CommandServiceAckResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| ack(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::CommandServiceListRequest>,
    ) -> Result<Response<api::CommandServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }

    async fn pending(
        &self,
        req: Request<api::CommandServicePendingRequest>,
    ) -> Result<Response<api::CommandServicePendingResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| pending(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn update(
        &self,
        req: Request<api::CommandServiceUpdateRequest>,
    ) -> Result<Response<api::CommandServiceUpdateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta.into(), write).scope_boxed())
            .await
    }
}

async fn ack(
    req: api::CommandServiceAckRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::CommandServiceAckResponse, Error> {
    let id = req.command_id.parse().map_err(Error::ParseId)?;
    let command = Command::by_id(id, &mut write).await?;

    let resource: Resource = command.node_id.map_or(command.host_id.into(), Into::into);
    let authz = write.auth_for(&meta, CommandPerm::Ack, resource).await?;

    if command.acked_at.is_none() {
        command.ack(&mut write).await?;
    } else {
        warn!("Duplicate ack for command id: {0}", command.id);
    }

    if let Some(node) = command.node(&mut write).await? {
        ack_next_state(node, &command, &authz, &mut write).await?;
    }

    Ok(api::CommandServiceAckResponse {})
}

async fn ack_next_state(
    node: Node,
    command: &Command,
    authz: &AuthZ,
    write: &mut WriteConn<'_, '_>,
) -> Result<(), Error> {
    use CommandType::*;
    let node_state = if let Some(next) = node.next_state {
        match (command.command_type, next) {
            (NodeCreate | NodeStart | NodeRestart, NextState::Starting) => NodeState::Starting,
            (NodeStop, NextState::Stopping) => NodeState::Stopped,
            (NodeUpgrade, NextState::Upgrading) => NodeState::Upgrading,
            (NodeDelete, NextState::Deleting) => NodeState::Deleting,
            _ => return Ok(()),
        }
    } else {
        return Ok(());
    };

    let update = UpdateNodeState {
        id: node.id,
        node_state: Some(node_state),
        next_state: Some(None),
        protocol_state: None,
        protocol_health: None,
        p2p_address: None,
    };
    let node = update.apply(write).await?;

    let node = api::Node::from_model(node, authz, write)
        .await
        .map_err(|err| Error::GrpcHost(Box::new(err)))?;
    let updated_by = common::Resource::from(authz);
    let msg = api::NodeMessage::updated(node, updated_by);
    write.mqtt(msg);

    Ok(())
}

async fn list(
    req: api::CommandServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CommandServiceListResponse, Error> {
    let filter = CommandFilter {
        node_id: req
            .node_id
            .as_deref()
            .map(|id| id.parse().map_err(Error::ParseNodeId))
            .transpose()?,
        host_id: req
            .host_id
            .as_deref()
            .map(|id| id.parse().map_err(Error::ParseHostId))
            .transpose()?,
        exit_code: req
            .exit_code
            .map(|_| Option::from(req.exit_code()).ok_or(Error::UnknownExitCode))
            .transpose()?,
    };

    let resources: Resources = match (filter.node_id, filter.host_id) {
        (Some(node_id), Some(host_id)) => [Resource::from(node_id), Resource::from(host_id)].into(),
        (Some(node_id), None) => [node_id].into(),
        (None, Some(host_id)) => [host_id].into(),
        (None, None) => return Err(Error::ListMissingNodeOrHost)?,
    };

    let authz = read
        .auth_or_for(&meta, CommandAdminPerm::List, CommandPerm::List, resources)
        .await?;

    let filtered = Command::filter(filter, &mut read).await?;
    let mut commands = Vec::with_capacity(filtered.len());
    for command in filtered {
        commands.push(api::Command::from(&command, None, &authz, &mut read).await?);
    }

    Ok(api::CommandServiceListResponse { commands })
}

async fn pending(
    req: api::CommandServicePendingRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CommandServicePendingResponse, Error> {
    let host_id = req.host_id.parse().map_err(Error::ParseHostId)?;
    let authz = read
        .auth_or_for(
            &meta,
            CommandAdminPerm::Pending,
            CommandPerm::Pending,
            host_id,
        )
        .await?;

    let pending = Command::host_pending(host_id, &mut read).await?;
    let mut commands = Vec::with_capacity(pending.len());
    for command in pending {
        commands.push(api::Command::from(&command, None, &authz, &mut read).await?);
    }

    Ok(api::CommandServicePendingResponse { commands })
}

async fn update(
    req: api::CommandServiceUpdateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::CommandServiceUpdateResponse, Error> {
    let id = req.command_id.parse().map_err(Error::ParseId)?;
    let command = Command::by_id(id, &mut write).await?;

    let mut resources = vec![Resource::from(command.host_id)];
    if let Some(node_id) = command.node_id {
        resources.push(Resource::from(node_id));
    }
    let authz = write
        .auth_for(&meta, CommandPerm::Update, &resources)
        .await?;

    let update = UpdateCommand {
        id: req.command_id.parse().map_err(Error::ParseId)?,
        exit_code: req.exit_code().into(),
        exit_message: req.exit_message,
        retry_hint_seconds: req
            .retry_hint_seconds
            .map(|secs| secs.try_into().map_err(Error::RetryHint))
            .transpose()?,
        completed_at: req.exit_code.map(|_| chrono::Utc::now()),
    };
    let updated = update.apply(&mut write).await?;
    let cmd = api::Command::from(&updated, None, &authz, &mut write).await?;
    write.mqtt(cmd.clone());

    match updated.exit_code {
        Some(ExitCode::Ok) => success::confirm(&updated, &authz, &mut write).await?,
        Some(_) => recover::recover(&updated, &authz, &mut write)
            .await?
            .into_iter()
            .for_each(|cmd| write.mqtt(cmd)),
        None => (),
    };

    Ok(api::CommandServiceUpdateResponse { command: Some(cmd) })
}

impl api::Command {
    pub async fn from(
        command: &Command,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        match command.command_type {
            CommandType::HostStart => host_start(command, org_id, conn).await,
            CommandType::HostStop => host_stop(command, org_id, conn).await,
            CommandType::HostRestart => host_restart(command, org_id, conn).await,
            CommandType::HostPending => host_pending(command, org_id, conn).await,
            CommandType::NodeCreate => node_create(command, org_id, authz, conn).await,
            CommandType::NodeStart => node_start(command, org_id, conn).await,
            CommandType::NodeStop => node_stop(command, org_id, conn).await,
            CommandType::NodeRestart => node_restart(command, org_id, conn).await,
            CommandType::NodeUpdate => unimplemented!(),
            CommandType::NodeUpgrade => node_upgrade(command, org_id, authz, conn).await,
            CommandType::NodeDelete => node_delete(command, org_id, conn).await,
        }
    }
}

/// Create a new `api::HostCommand` from a `Command`.
fn host_command(
    command: &Command,
    host: Host,
    host_cmd: api::host_command::Command,
) -> Result<api::Command, Error> {
    let exit_code = command
        .exit_code
        .map(|code| api::CommandExitCode::from(code).into());
    let retry_hint_seconds = command
        .retry_hint_seconds
        .map(|hint| hint.try_into().map_err(Error::RetryHint))
        .transpose()?;

    Ok(api::Command {
        command_id: command.id.to_string(),
        exit_code,
        exit_message: command.exit_message.clone(),
        retry_hint_seconds,
        created_at: Some(NanosUtc::from(command.created_at).into()),
        acked_at: command.acked_at.map(NanosUtc::from).map(Into::into),
        command: Some(api::command::Command::Host(api::HostCommand {
            host_id: command.host_id.to_string(),
            network_name: host.network_name,
            command: Some(host_cmd),
        })),
    })
}

async fn host_start(
    command: &Command,
    org_id: Option<OrgId>,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let host = Host::by_id(command.host_id, org_id, conn).await?;
    let host_cmd = api::host_command::Command::Start(api::HostStart {});
    host_command(command, host, host_cmd)
}

async fn host_stop(
    command: &Command,
    org_id: Option<OrgId>,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let host = Host::by_id(command.host_id, org_id, conn).await?;
    let host_cmd = api::host_command::Command::Stop(api::HostStop {});
    host_command(command, host, host_cmd)
}

async fn host_restart(
    command: &Command,
    org_id: Option<OrgId>,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let host = Host::by_id(command.host_id, org_id, conn).await?;
    let host_cmd = api::host_command::Command::Restart(api::HostRestart {});
    host_command(command, host, host_cmd)
}

pub async fn host_pending(
    command: &Command,
    org_id: Option<OrgId>,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let host = Host::by_id(command.host_id, org_id, conn).await?;
    let host_cmd = api::host_command::Command::Pending(api::HostPending {});
    host_command(command, host, host_cmd)
}

/// Create a new `api::NodeCommand` from a `Command`.
fn node_command(
    command: &Command,
    node: Node,
    host: Host,
    node_cmd: api::node_command::Command,
) -> Result<api::Command, Error> {
    let exit_code = command
        .exit_code
        .map(|code| api::CommandExitCode::from(code).into());
    let retry_hint_seconds = command
        .retry_hint_seconds
        .map(|hint| hint.try_into().map_err(Error::RetryHint))
        .transpose()?;

    Ok(api::Command {
        command_id: command.id.to_string(),
        exit_code,
        exit_message: command.exit_message.clone(),
        retry_hint_seconds,
        created_at: Some(NanosUtc::from(command.created_at).into()),
        acked_at: command.acked_at.map(NanosUtc::from).map(Into::into),
        command: Some(api::command::Command::Node(api::NodeCommand {
            host_id: host.id.to_string(),
            network_name: host.network_name,
            node_id: node.id.to_string(),
            node_name: node.node_name,
            command: Some(node_cmd),
        })),
    })
}

async fn node_create(
    command: &Command,
    org_id: Option<OrgId>,
    authz: &AuthZ,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node = Node::by_id(node_id, conn).await?;
    let host = Host::by_id(node.host_id, org_id, conn).await?;

    let api_node = api::Node::from_model(node.clone(), authz, conn)
        .await
        .map_err(|err| Error::NodeResponse(Box::new(err)))?;

    let node_cmd = api::node_command::Command::Create(api::NodeCreate {
        node: Some(api_node),
    });

    node_command(command, node, host, node_cmd)
}

async fn node_start(
    command: &Command,
    org_id: Option<OrgId>,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node = Node::by_id(node_id, conn).await?;
    let host = Host::by_id(node.host_id, org_id, conn).await?;
    let node_cmd = api::node_command::Command::Start(api::NodeStart {});
    node_command(command, node, host, node_cmd)
}

async fn node_stop(
    command: &Command,
    org_id: Option<OrgId>,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node = Node::by_id(node_id, conn).await?;
    let host = Host::by_id(node.host_id, org_id, conn).await?;
    let node_cmd = api::node_command::Command::Stop(api::NodeStop {});
    node_command(command, node, host, node_cmd)
}

async fn node_restart(
    command: &Command,
    org_id: Option<OrgId>,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node = Node::by_id(node_id, conn).await?;
    let host = Host::by_id(node.host_id, org_id, conn).await?;
    let node_cmd = api::node_command::Command::Restart(api::NodeRestart {});
    node_command(command, node, host, node_cmd)
}

pub async fn node_update(
    command: &Command,
    org_id: Option<OrgId>,
    update: api::NodeUpdate,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node = Node::by_id(node_id, conn).await?;
    let host = Host::by_id(node.host_id, org_id, conn).await?;
    let node_cmd = api::node_command::Command::Update(update);
    node_command(command, node, host, node_cmd)
}

async fn node_upgrade(
    command: &Command,
    org_id: Option<OrgId>,
    authz: &AuthZ,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node = Node::by_id(node_id, conn).await?;
    let host = Host::by_id(node.host_id, org_id, conn).await?;

    let api_node = api::Node::from_model(node.clone(), authz, conn)
        .await
        .map_err(|err| Error::NodeResponse(Box::new(err)))?;

    let node_cmd = api::node_command::Command::Upgrade(api::NodeUpgrade {
        node: Some(api_node),
    });

    node_command(command, node, host, node_cmd)
}

pub async fn node_delete(
    command: &Command,
    org_id: Option<OrgId>,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node = Node::deleted_by_id(node_id, conn).await?;
    let host = Host::by_id(node.host_id, org_id, conn).await?;
    let node_cmd = api::node_command::Command::Delete(api::NodeDelete {});
    node_command(command, node, host, node_cmd)
}
