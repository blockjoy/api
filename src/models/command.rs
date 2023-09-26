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

use super::schema::{commands, sql_types};
use super::{Host, Node};

type Pending = dsl::Filter<commands::table, dsl::IsNull<commands::exit_status>>;

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
    /// Failed to update command: {0}
    Update(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            DeletePending(NotFound) | FindById(_, NotFound) | FindPending(NotFound) => {
                Status::not_found("Not found.")
            }
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
    pub sub_cmd: Option<String>,
    pub response: Option<String>,
    pub exit_status: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub node_id: Option<NodeId>,
    pub acked_at: Option<DateTime<Utc>>,
}

impl Command {
    pub async fn find_by_id(id: CommandId, conn: &mut Conn<'_>) -> Result<Self, Error> {
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
        Host::find_by_id(self.host_id, conn)
            .await
            .map_err(Error::Host)
    }

    pub async fn node(&self, conn: &mut Conn<'_>) -> Result<Option<Node>, Error> {
        match self.node_id {
            Some(node_id) => Ok(Some(Node::find_by_id(node_id, conn).await?)),
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
        commands::table.filter(commands::exit_status.is_null())
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = commands)]
pub struct NewCommand<'a> {
    pub host_id: HostId,
    pub cmd: CommandType,
    pub sub_cmd: Option<&'a str>,
    pub node_id: Option<NodeId>,
}

impl NewCommand<'_> {
    pub fn from(host_id: HostId, cmd: CommandType) -> Self {
        NewCommand {
            host_id,
            cmd,
            sub_cmd: None,
            node_id: None,
        }
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
pub struct UpdateCommand<'a> {
    pub id: CommandId,
    pub response: Option<&'a str>,
    pub exit_status: Option<i32>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl UpdateCommand<'_> {
    pub async fn update(self, conn: &mut Conn<'_>) -> Result<Command, Error> {
        diesel::update(commands::table.find(self.id))
            .set(self)
            .get_result::<Command>(conn)
            .await
            .map_err(Error::Update)
    }
}
