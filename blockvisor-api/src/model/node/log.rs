use chrono::{DateTime, Utc};
use diesel::deserialize::{FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::sql_types::Jsonb;
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel::serialize::{Output, ToSql};
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::{HostId, NodeId, OrgId, Resource, ResourceId, ResourceType};
use crate::auth::AuthZ;
use crate::database::Conn;
use crate::grpc::Status;
use crate::model::schema::{node_logs, sql_types};
use crate::model::ImageId;

use super::Node;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create new node log: {0}
    Create(diesel::result::Error),
    /// Failed to find node log for node id `{0}`: {1}
    ByNodeId(NodeId, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("Node log already exists.")
            }
            ByNodeId(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

/// An append-only log of events over the lifetime of a node.
#[derive(Debug, Queryable)]
pub struct NodeLog {
    pub id: Uuid,
    pub node_id: NodeId,
    pub host_id: HostId,
    pub event: NodeEvent,
    pub event_data: Option<NodeEventData>,
    pub created_by_type: ResourceType,
    pub created_by_id: ResourceId,
    pub created_at: DateTime<Utc>,
}

impl NodeLog {
    pub async fn by_node_id(node_id: NodeId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        node_logs::table
            .filter(node_logs::node_id.eq(node_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByNodeId(node_id, err))
    }
}

#[derive(Insertable)]
#[diesel(table_name = node_logs)]
pub struct NewNodeLog {
    pub node_id: NodeId,
    pub host_id: HostId,
    pub event: NodeEvent,
    pub event_data: Option<NodeEventData>,
    pub created_by_type: ResourceType,
    pub created_by_id: ResourceId,
    pub created_at: DateTime<Utc>,
}

impl NewNodeLog {
    pub fn new(node_id: NodeId, host_id: HostId, authz: &AuthZ, event: LogEvent) -> Self {
        let created_by = Resource::from(authz);
        let (event, event_data) = event.split();

        NewNodeLog {
            node_id,
            host_id,
            event,
            event_data,
            created_by_type: created_by.typ(),
            created_by_id: created_by.id(),
            created_at: Utc::now(),
        }
    }

    pub fn from(node: &Node, authz: &AuthZ, event: LogEvent) -> Self {
        Self::new(node.id, node.host_id, authz, event)
    }

    pub async fn create(self, conn: &mut Conn<'_>) -> Result<NodeLog, Error> {
        diesel::insert_into(node_logs::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}

#[derive(Clone, Debug)]
pub enum LogEvent {
    /// A `NodeCreate` message has been sent to blockvisord.
    ///
    /// This should be followed by `CreateSucceeded` or `CreateFailed`.
    CreateStarted,
    /// Confirmation by blockvisord that the node was created.
    CreateSucceeded,
    /// Notification by blockvisord that the node was not created.
    ///
    /// The API will send a `NodeDelete` message to clean up the resources, and
    /// either retry on another host (followed by `CreateStarted`) or abort
    /// (followed by `CreateCancelled`).
    CreateFailed,
    /// Node creation was cancelled because of some non-transient failure.
    CreateCancelled,
    /// This node was transferred to another org.
    OrgTransferred(OrgTransferred),
    /// A `NodeUpgrade` message has been sent to blockvisord.
    ///
    /// This should be followed by `UpgradeSucceeded` or `UpgradeFailed`.
    UpgradeStarted(UpgradeStarted),
    /// Confirmation that a node was successfully upgraded.
    UpgradeSucceeded,
    /// Notification that an attempt to upgrade a node failed.
    UpgradeFailed,
}

impl LogEvent {
    pub const fn split(self) -> (NodeEvent, Option<NodeEventData>) {
        match self {
            LogEvent::CreateStarted => (NodeEvent::CreateStarted, None),
            LogEvent::CreateSucceeded => (NodeEvent::CreateSucceeded, None),
            LogEvent::CreateFailed => (NodeEvent::CreateFailed, None),
            LogEvent::CreateCancelled => (NodeEvent::CreateCancelled, None),
            LogEvent::OrgTransferred(data) => (
                NodeEvent::OrgTransferred,
                Some(NodeEventData::OrgTransferred(data)),
            ),
            LogEvent::UpgradeStarted(data) => (
                NodeEvent::UpgradeStarted,
                Some(NodeEventData::UpgradeStarted(data)),
            ),
            LogEvent::UpgradeSucceeded => (NodeEvent::UpgradeSucceeded, None),
            LogEvent::UpgradeFailed => (NodeEvent::UpgradeFailed, None),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct OrgTransferred {
    pub old: OrgId,
    pub new: OrgId,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct UpgradeStarted {
    pub old: ImageId,
    pub new: ImageId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeEvent"]
pub enum NodeEvent {
    CreateStarted,
    CreateSucceeded,
    CreateFailed,
    CreateCancelled,
    OrgTransferred,
    UpgradeStarted,
    UpgradeSucceeded,
    UpgradeFailed,
}

#[derive(Clone, Copy, Debug, AsExpression, FromSqlRow, Serialize, Deserialize)]
#[diesel(sql_type = Jsonb)]
pub enum NodeEventData {
    OrgTransferred(OrgTransferred),
    UpgradeStarted(UpgradeStarted),
}

impl FromSql<Jsonb, Pg> for NodeEventData {
    fn from_sql(value: PgValue<'_>) -> diesel::deserialize::Result<Self> {
        serde_json::from_value(FromSql::<Jsonb, Pg>::from_sql(value)?).map_err(Into::into)
    }
}

impl ToSql<Jsonb, Pg> for NodeEventData {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        let json = serde_json::to_value(self).unwrap();
        <serde_json::Value as ToSql<Jsonb, Pg>>::to_sql(&json, &mut out.reborrow())
    }
}
