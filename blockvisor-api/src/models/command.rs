use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr};
use diesel::dsl;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use tonic::Status;
use uuid::Uuid;

use crate::auth::resource::{HostId, NodeId};
use crate::database::Conn;
use crate::grpc::api;

use super::schema::{commands, sql_types};
use super::{Host, Node};

type Pending = dsl::Filter<commands::table, dsl::IsNull<commands::exit_code>>;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to ack command: {0}
    Ack(diesel::result::Error),
    /// Failed to create new command: {0}
    Create(diesel::result::Error),
    /// Failed to delete pending commands: {0}
    DeletePending(diesel::result::Error),
    /// Failed to find command by id `{0}`: {1}
    FindById(CommandId, diesel::result::Error),
    /// Failed to find pending commands: {0}
    FindPending(diesel::result::Error),
    /// Command Host error: {0}
    Host(#[from] super::host::Error),
    /// Command Node error: {0}
    Node(#[from] super::node::Error),
    /// Failed to parse CommandId: {0}
    ParseId(uuid::Error),
    /// Failed to parse `retry_hint_seconds` as u64: {0}
    RetryHint(std::num::TryFromIntError),
    /// Failed to update command: {0}
    Update(diesel::result::Error),
    /// Attempt to create a command meant for a node without specificying a node id.
    NodeCommandWithoutNodeId,
    /// Attempt to create a command meant for a host while also specificying a node id.
    HostCommandWithNodeId,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            DeletePending(NotFound) | FindById(_, NotFound) | FindPending(NotFound) => {
                Status::not_found("Not found.")
            }
            ParseId(_) => Status::invalid_argument("id"),
            RetryHint(_) => Status::invalid_argument("retry_hint_seconds"),
            Host(err) => err.into(),
            Node(err) => err.into(),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumHostCmd"]
pub enum CommandType {
    CreateNode,
    RestartNode,
    KillNode,
    ShutdownNode,
    DeleteNode,
    UpdateNode,
    MigrateNode,
    UpgradeNode,
    GetNodeVersion,
    GetBVSVersion,
    CreateBVS,
    UpdateBVS,
    RestartBVS,
    RemoveBVS,
    StopBVS,
}

impl CommandType {
    /// Returns true if this command is directed at the host.
    pub const fn host_command(self) -> bool {
        use CommandType::*;

        match self {
            CreateNode | RestartNode | KillNode | ShutdownNode | DeleteNode | UpdateNode
            | MigrateNode | UpgradeNode | GetNodeVersion => false,
            GetBVSVersion | CreateBVS | UpdateBVS | RestartBVS | RemoveBVS | StopBVS => true,
        }
    }

    /// Returns true if this command is directed at a specific node on the host.
    pub const fn node_command(self) -> bool {
        !self.host_command()
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    DieselNewType,
    Deref,
    From,
    FromStr,
)]
pub struct CommandId(Uuid);

#[derive(Clone, Debug, Queryable, Identifiable)]
pub struct Command {
    pub id: CommandId,
    pub host_id: HostId,
    pub cmd: CommandType,
    pub exit_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub node_id: Option<NodeId>,
    pub acked_at: Option<DateTime<Utc>>,
    pub retry_hint_seconds: Option<i64>,
    pub exit_code: Option<ExitCode>,
}

impl Command {
    pub async fn by_id(id: CommandId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        commands::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn find_pending_by_host(
        host_id: HostId,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Command>, Error> {
        Self::pending()
            .filter(commands::host_id.eq(host_id))
            .order_by(commands::created_at.asc())
            .get_results(conn)
            .await
            .map_err(Error::FindPending)
    }

    pub async fn delete_pending(node_id: NodeId, conn: &mut Conn<'_>) -> Result<(), Error> {
        diesel::delete(Self::pending().filter(commands::node_id.eq(node_id)))
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(Error::DeletePending)
    }

    pub async fn host(&self, conn: &mut Conn<'_>) -> Result<Host, Error> {
        Host::by_id(self.host_id, conn).await.map_err(Error::Host)
    }

    pub async fn node(&self, conn: &mut Conn<'_>) -> Result<Option<Node>, Error> {
        match self.node_id {
            Some(node_id) => Ok(Some(Node::by_id(node_id, conn).await?)),
            None => Ok(None),
        }
    }

    pub async fn ack(&self, conn: &mut Conn<'_>) -> Result<(), Error> {
        diesel::update(commands::table.find(self.id))
            .set(commands::acked_at.eq(chrono::Utc::now()))
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(Error::Ack)
    }

    fn pending() -> Pending {
        commands::table.filter(commands::exit_code.is_null())
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = commands)]
pub struct NewCommand {
    host_id: HostId,
    cmd: CommandType,
    node_id: Option<NodeId>,
}

impl NewCommand {
    pub const fn host(host: &Host, cmd: CommandType) -> Result<Self, Error> {
        if !cmd.host_command() {
            return Err(Error::NodeCommandWithoutNodeId);
        }
        Ok(NewCommand {
            host_id: host.id,
            cmd,
            node_id: None,
        })
    }

    pub const fn node(node: &Node, cmd: CommandType) -> Result<Self, Error> {
        if !cmd.node_command() {
            return Err(Error::HostCommandWithNodeId);
        }
        Ok(NewCommand {
            host_id: node.host_id,
            cmd,
            node_id: Some(node.id),
        })
    }

    pub async fn create(self, conn: &mut Conn<'_>) -> Result<Command, Error> {
        diesel::insert_into(commands::table)
            .values(self)
            .get_result::<Command>(conn)
            .await
            .map_err(Error::Create)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = commands)]
pub struct UpdateCommand {
    pub id: CommandId,
    pub exit_code: Option<ExitCode>,
    pub exit_message: Option<String>,
    pub retry_hint_seconds: Option<i64>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl UpdateCommand {
    pub fn from_request(request: api::CommandServiceUpdateRequest) -> Result<Self, Error> {
        Ok(UpdateCommand {
            id: request.id.parse().map_err(Error::ParseId)?,
            exit_code: request.exit_code().into(),
            exit_message: request.exit_message,
            retry_hint_seconds: request
                .retry_hint_seconds
                .map(|secs| secs.try_into().map_err(Error::RetryHint))
                .transpose()?,
            completed_at: request.exit_code.map(|_| chrono::Utc::now()),
        })
    }

    pub async fn update(self, conn: &mut Conn<'_>) -> Result<Command, Error> {
        diesel::update(commands::table.find(self.id))
            .set(self)
            .get_result(conn)
            .await
            .map_err(Error::Update)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumCommandExitCode"]
pub enum ExitCode {
    Ok,
    InternalError,
    NodeNotFound,
    BlockingJobRunning,
    ServiceNotReady,
    ServiceBroken,
    NotSupported,
}

impl From<api::CommandExitCode> for Option<ExitCode> {
    fn from(code: api::CommandExitCode) -> Self {
        match code {
            api::CommandExitCode::Unspecified => None,
            api::CommandExitCode::Ok => Some(ExitCode::Ok),
            api::CommandExitCode::InternalError => Some(ExitCode::InternalError),
            api::CommandExitCode::NodeNotFound => Some(ExitCode::NodeNotFound),
            api::CommandExitCode::BlockingJobRunning => Some(ExitCode::BlockingJobRunning),
            api::CommandExitCode::ServiceNotReady => Some(ExitCode::ServiceNotReady),
            api::CommandExitCode::ServiceBroken => Some(ExitCode::ServiceBroken),
            api::CommandExitCode::NotSupported => Some(ExitCode::NotSupported),
        }
    }
}

impl From<ExitCode> for api::CommandExitCode {
    fn from(code: ExitCode) -> Self {
        match code {
            ExitCode::Ok => api::CommandExitCode::Ok,
            ExitCode::InternalError => api::CommandExitCode::InternalError,
            ExitCode::NodeNotFound => api::CommandExitCode::NodeNotFound,
            ExitCode::BlockingJobRunning => api::CommandExitCode::BlockingJobRunning,
            ExitCode::ServiceNotReady => api::CommandExitCode::ServiceNotReady,
            ExitCode::ServiceBroken => api::CommandExitCode::ServiceBroken,
            ExitCode::NotSupported => api::CommandExitCode::NotSupported,
        }
    }
}
