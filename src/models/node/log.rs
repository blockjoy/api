use std::collections::HashMap;

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use displaydoc::Display;
use thiserror::Error;
use tonic::Status;
use uuid::Uuid;

use crate::auth::resource::{HostId, NodeId};
use crate::database::Conn;
use crate::models::schema::{node_logs, sql_types};
use crate::models::Host;

use super::{Node, NodeType, NodeVersion};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create new node log: {0}
    Create(diesel::result::Error),
    /// Failed to find node log by node: {0}
    ByNode(diesel::result::Error),
    /// Failed to find recent node logs by node: {0}
    ByNodeSince(diesel::result::Error),
    /// Node log host error: {0}
    Host(#[from] crate::models::host::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(_) => Status::already_exists("Already exists."),
            ByNode(_) | ByNodeSince(_) => Status::not_found("Not found."),
            Host(err) => err.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeLogEvent"]
pub enum NodeLogEvent {
    /// This variant is used to note that a `NodeCreate` message has been sent to blockvisord. There
    /// should be a `Succeeded` or a `Failed` noted afterwards.
    Created,
    /// This variant is used to note that node was successfully created, and that the create was
    /// confirmed to be successful by blockvisord.
    Succeeded,
    /// This variant is used to note that a node was not created. When we receive this variant, we
    /// will send a `NodeDelete` message to blockvisord to clean up, and this message should either
    /// be followed by a `Created` or a `Canceled` log entry, depending on whether we dediced to
    /// retry or to abort.
    Failed,
    /// This variant is used to note that we aborted from creating the node, because the failure we
    /// ran into was endemic.
    Canceled,
}

/// Records of this table indicate that some event related to node deployments has happened. Note
/// that there is some redundancy in this table, because we want to be able to keep this log
/// meaningful as records are deleted from the `nodes` table.
#[derive(Debug, Queryable)]
pub struct NodeLog {
    pub id: Uuid,
    pub host_id: HostId,
    pub node_id: NodeId,
    pub event: NodeLogEvent,
    pub blockchain_name: String,
    pub node_type: NodeType,
    pub version: String,
    pub created_at: DateTime<Utc>,
}

impl NodeLog {
    pub async fn by_node(node: &Node, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        node_logs::table
            .filter(node_logs::node_id.eq(node.id))
            .get_results(conn)
            .await
            .map_err(Error::ByNode)
    }

    /// Finds all deployments belonging to the provided node, that were created after the provided
    /// date.
    pub async fn by_node_since(
        node: &Node,
        since: DateTime<Utc>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        node_logs::table
            .filter(node_logs::node_id.eq(node.id))
            .filter(node_logs::created_at.gt(since))
            .get_result(conn)
            .await
            .map_err(Error::ByNodeSince)
    }

    /// Returns the number of distinct hosts we have tried to deploy a node on. To do this it counts
    /// the number of `CreateSent` events that were undertaken.
    pub async fn hosts_tried(
        deployments: &[Self],
        conn: &mut Conn<'_>,
    ) -> Result<Vec<(Host, usize)>, Error> {
        let mut counts: HashMap<HostId, usize> = HashMap::new();
        for deployment in deployments {
            *counts.entry(deployment.host_id).or_insert(0) += 1;
        }
        let host_ids = counts.keys().copied().collect();
        let hosts = Host::find_by_ids(host_ids, conn).await?;
        let hosts = hosts
            .into_iter()
            .map(|h @ Host { id, .. }| (h, counts[&id]))
            .collect();
        Ok(hosts)
    }

    // Do not add update or delete here, this table is meant as a log and is therefore append-only.
}

#[derive(Insertable)]
#[diesel(table_name = node_logs)]
pub struct NewNodeLog<'a> {
    pub host_id: HostId,
    pub node_id: NodeId,
    pub event: NodeLogEvent,
    pub blockchain_name: &'a str,
    pub node_type: NodeType,
    pub version: NodeVersion,
    pub created_at: DateTime<Utc>,
}

impl NewNodeLog<'_> {
    pub async fn create(self, conn: &mut Conn<'_>) -> Result<NodeLog, Error> {
        diesel::insert_into(node_logs::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}
