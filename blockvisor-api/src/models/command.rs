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
    /// Failed to delete pending host commands: {0}
    DeleteHostPending(diesel::result::Error),
    /// Failed to delete pending node commands: {0}
    DeleteNodePending(diesel::result::Error),
    /// Failed to filter commands: {0}
    Filter(diesel::result::Error),
    /// Failed to find command by id `{0}`: {1}
    FindById(CommandId, diesel::result::Error),
    /// Failed to check for pending host commands: {0}
    HasHostPending(diesel::result::Error),
    /// Command Host error: {0}
    Host(#[from] super::host::Error),
    /// Failed to find pending host commands: {0}
    HostPending(diesel::result::Error),
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
            DeleteHostPending(NotFound)
            | DeleteNodePending(NotFound)
            | FindById(_, NotFound)
            | HasHostPending(NotFound)
            | HostPending(NotFound) => Status::not_found("Not found."),
            ParseId(_) => Status::invalid_argument("id"),
            RetryHint(_) => Status::invalid_argument("retry_hint_seconds"),
            Host(err) => err.into(),
            Node(err) => err.into(),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumCommandType"]
pub enum CommandType {
    HostStart,
    HostStop,
    HostRestart,
    HostPending,
    NodeCreate,
    NodeStart,
    NodeStop,
    NodeRestart,
    NodeUpgrade,
    NodeUpdate,
    NodeDelete,
}

impl CommandType {
    const fn is_host(self) -> bool {
        use CommandType::*;
        matches!(self, HostStart | HostStop | HostRestart | HostPending)
    }

    const fn is_node(self) -> bool {
        use CommandType::*;
        matches!(
            self,
            NodeCreate | NodeStart | NodeStop | NodeRestart | NodeUpgrade | NodeUpdate | NodeDelete
        )
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
    pub exit_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub node_id: Option<NodeId>,
    pub acked_at: Option<DateTime<Utc>>,
    pub retry_hint_seconds: Option<i64>,
    pub exit_code: Option<ExitCode>,
    pub command_type: CommandType,
}

impl Command {
    pub async fn by_id(id: CommandId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        commands::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn has_host_pending(host_id: HostId, conn: &mut Conn<'_>) -> Result<bool, Error> {
        let pending = Self::pending().filter(commands::host_id.eq(host_id));

        diesel::select(dsl::exists(pending))
            .get_result(conn)
            .await
            .map_err(Error::HasHostPending)
    }

    pub async fn host_pending(host_id: HostId, conn: &mut Conn<'_>) -> Result<Vec<Command>, Error> {
        Self::pending()
            .filter(commands::host_id.eq(host_id))
            .order_by(commands::created_at.asc())
            .get_results(conn)
            .await
            .map_err(Error::HostPending)
    }

    pub async fn filter(filter: CommandFilter, conn: &mut Conn<'_>) -> Result<Vec<Command>, Error> {
        let mut query = commands::table.into_boxed();

        if let Some(host_id) = filter.host_id {
            query = query.filter(commands::host_id.eq(host_id));
        }
        if let Some(node_id) = filter.node_id {
            query = query.filter(commands::node_id.eq(node_id));
        }
        if let Some(exit_code) = filter.exit_code {
            query = query.filter(commands::exit_code.eq(exit_code));
        }

        query
            .order_by(commands::created_at.desc())
            .get_results(conn)
            .await
            .map_err(Error::Filter)
    }

    pub async fn delete_host_pending(host_id: HostId, conn: &mut Conn<'_>) -> Result<(), Error> {
        let pending = Self::pending().filter(commands::host_id.eq(host_id));

        diesel::delete(pending)
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(Error::DeleteHostPending)
    }

    pub async fn delete_node_pending(node_id: NodeId, conn: &mut Conn<'_>) -> Result<(), Error> {
        let pending = Self::pending().filter(commands::node_id.eq(node_id));

        diesel::delete(pending)
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(Error::DeleteNodePending)
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
    node_id: Option<NodeId>,
    command_type: CommandType,
}

impl NewCommand {
    pub const fn host(host_id: HostId, command_type: CommandType) -> Result<Self, Error> {
        if !command_type.is_host() {
            return Err(Error::NodeCommandWithoutNodeId);
        }

        Ok(NewCommand {
            host_id,
            node_id: None,
            command_type,
        })
    }

    pub const fn node(node: &Node, command_type: CommandType) -> Result<Self, Error> {
        if !command_type.is_node() {
            return Err(Error::HostCommandWithNodeId);
        }

        Ok(NewCommand {
            host_id: node.host_id,
            node_id: Some(node.id),
            command_type,
        })
    }

    pub async fn create(self, conn: &mut Conn<'_>) -> Result<Command, Error> {
        diesel::insert_into(commands::table)
            .values(self)
            .get_result(conn)
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

pub struct CommandFilter {
    pub node_id: Option<NodeId>,
    pub host_id: Option<HostId>,
    pub exit_code: Option<ExitCode>,
}
