use crate::models::schema::commands;
use crate::Result;
use chrono::{DateTime, Utc};
use diesel::{dsl, prelude::*};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumHostCmd"]
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

#[derive(Clone, Debug, Queryable, Identifiable)]
pub struct Command {
    pub id: Uuid,
    pub host_id: Uuid,
    pub cmd: CommandType,
    pub sub_cmd: Option<String>,
    pub response: Option<String>,
    pub exit_status: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub node_id: Option<Uuid>,
}

type Pending = dsl::Filter<commands::table, dsl::IsNull<commands::exit_status>>;

impl Command {
    pub async fn find_by_id(id: Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let cmd = commands::table.find(id).get_result(conn).await?;
        Ok(cmd)
    }

    pub async fn find_pending_by_host(
        host_id: Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<Command>> {
        let commands = Self::pending()
            .filter(commands::host_id.eq(host_id))
            .order_by(commands::created_at.asc())
            .get_results(conn)
            .await?;
        Ok(commands)
    }

    pub async fn delete_pending(node_id: uuid::Uuid, conn: &mut AsyncPgConnection) -> Result<()> {
        diesel::delete(Self::pending().filter(commands::node_id.eq(node_id)))
            .execute(conn)
            .await?;
        Ok(())
    }

    pub async fn host(&self, conn: &mut AsyncPgConnection) -> Result<super::Host> {
        super::Host::find_by_id(self.host_id, conn).await
    }

    pub async fn node(&self, conn: &mut AsyncPgConnection) -> Result<Option<super::Node>> {
        let Some(node_id) = self.node_id else { return Ok(None) };
        Ok(Some(super::Node::find_by_id(node_id, conn).await?))
    }

    fn pending() -> Pending {
        commands::table.filter(commands::exit_status.is_null())
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = commands)]
pub struct NewCommand<'a> {
    pub host_id: uuid::Uuid,
    pub cmd: CommandType,
    pub sub_cmd: Option<&'a str>,
    pub node_id: Option<Uuid>,
}

impl NewCommand<'_> {
    pub async fn create(self, conn: &mut AsyncPgConnection) -> Result<Command> {
        let cmd = diesel::insert_into(commands::table)
            .values(self)
            .get_result(conn)
            .await?;
        Ok(cmd)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = commands)]
pub struct UpdateCommand<'a> {
    pub id: Uuid,
    pub response: Option<&'a str>,
    pub exit_status: Option<i32>,
    pub completed_at: DateTime<Utc>,
}

impl UpdateCommand<'_> {
    pub async fn update(self, conn: &mut AsyncPgConnection) -> Result<Command> {
        let cmd = diesel::update(commands::table.find(self.id))
            .set(self)
            .get_result(conn)
            .await?;
        Ok(cmd)
    }
}
