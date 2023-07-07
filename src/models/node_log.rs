use std::collections::HashMap;

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::auth::resource::{HostId, NodeId};

use super::schema::node_logs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumNodeLogEvent"]
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
    pub id: uuid::Uuid,
    pub host_id: HostId,
    pub node_id: NodeId,
    pub event: NodeLogEvent,
    pub blockchain_name: String,
    pub node_type: super::NodeType,
    pub version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl NodeLog {
    pub async fn by_node(node: &super::Node, conn: &mut super::Conn) -> crate::Result<Vec<Self>> {
        let deployments = node_logs::table
            .filter(node_logs::node_id.eq(node.id))
            .get_results(conn)
            .await?;
        Ok(deployments)
    }

    /// Finds all deployments belonging to the provided node, that were created after the provided
    /// date.
    pub async fn by_node_since(
        node: &super::Node,
        since: chrono::DateTime<chrono::Utc>,
        conn: &mut super::Conn,
    ) -> crate::Result<Self> {
        let deployment = node_logs::table
            .filter(node_logs::node_id.eq(node.id))
            .filter(node_logs::created_at.gt(since))
            .get_result(conn)
            .await?;
        Ok(deployment)
    }

    /// Returns the number of distinct hosts we have tried to deploy a node on. To do this it counts
    /// the number of `CreateSent` events that were undertaken.
    pub async fn hosts_tried(
        deployments: &[Self],
        conn: &mut super::Conn,
    ) -> crate::Result<Vec<(super::Host, usize)>> {
        let mut counts: HashMap<HostId, usize> = HashMap::new();
        for deployment in deployments {
            *counts.entry(deployment.host_id).or_insert(0) += 1;
        }
        let host_ids: Vec<HostId> = counts.keys().copied().collect();
        let hosts = super::Host::by_ids(&host_ids, conn).await?;
        let hosts = hosts
            .into_iter()
            .map(|h @ super::Host { id, .. }| (h, counts[&id]))
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
    pub node_type: super::NodeType,
    pub version: &'a str,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl NewNodeLog<'_> {
    pub async fn create(self, conn: &mut super::Conn) -> crate::Result<NodeLog> {
        let deployment = diesel::insert_into(node_logs::table)
            .values(self)
            .get_result(conn)
            .await?;
        Ok(deployment)
    }
}
