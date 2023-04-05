use super::schema::node_deployment_logs;
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumNodeDeploymentAction"]
pub enum NodeDeploymentAction {
    /// A NodeDeploymentLog with this variant is created whenever we send a `NodeCreateCommand` to
    /// the
    CreateSent,
    SuccessReceived,
    FailureReceived,
}

/// Records of this table indicate that some action related to node deployments has happened. Note
/// that there is some redundancy in this table, because we want to be able to keep this log
/// meaningful as records are deleted from the `nodes` table.
#[derive(Debug, Queryable)]
pub struct NodeDeploymentLog {
    pub id: uuid::Uuid,
    pub host_id: uuid::Uuid,
    pub node_id: uuid::Uuid,
    pub action: NodeDeploymentAction,
    pub blockchain_id: uuid::Uuid,
    pub node_type: super::NodeType,
    pub version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl NodeDeploymentLog {
    pub async fn by_node(
        node: &super::Node,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let deployments = node_deployment_logs::table
            .filter(node_deployment_logs::node_id.eq(node.id))
            .get_results(conn)
            .await?;
        Ok(deployments)
    }

    /// Finds all deployments belonging to the provided node, that were created after the provided
    /// date.
    pub async fn by_node_since(
        node: &super::Node,
        since: chrono::DateTime<chrono::Utc>,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Self> {
        let deployment = node_deployment_logs::table
            .filter(node_deployment_logs::node_id.eq(node.id))
            .filter(node_deployment_logs::created_at.gt(since))
            .get_result(conn)
            .await?;
        Ok(deployment)
    }

    /// Returns the number of distinct hosts we have tried to deploy a node on. To do this it counts
    /// the number of `CreateSent` actions that were undertaken.
    pub fn n_hosts_tried(deployments: &[Self]) -> usize {
        let set: HashSet<_> = deployments
            .iter()
            .filter(|d| d.action == NodeDeploymentAction::CreateSent)
            .map(|d| d.host_id)
            .collect();
        set.len()
    }

    /// This function finds the host that was used for the most recent deploy, and then returns the
    /// number of times a `CreateNode` message was sent to that host. If the provided list is empty,
    /// it returns 0.
    pub fn n_deploys_tried_on_last_host(deployments: &[Self]) -> usize {
        let Some(last_host) = deployments
            .iter()
            .filter(|d| d.action == NodeDeploymentAction::CreateSent)
            .max_by_key(|h| h.created_at) else { return 0 };
        deployments
            .iter()
            .filter(|d| d.host_id == last_host.host_id)
            .count()
    }

    // Do not add update or delete here, this table is meant as a log and is therefore append-only.
}

#[derive(Insertable)]
#[diesel(table_name = node_deployment_logs)]
pub struct NewNodeDeploymentLog<'a> {
    pub host_id: uuid::Uuid,
    pub node_id: uuid::Uuid,
    pub action: NodeDeploymentAction,
    pub blockchain_id: uuid::Uuid,
    pub node_type: super::NodeType,
    pub version: &'a str,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl NewNodeDeploymentLog<'_> {
    pub async fn create(self, conn: &mut AsyncPgConnection) -> crate::Result<NodeDeploymentLog> {
        let deployment = diesel::insert_into(node_deployment_logs::table)
            .values(self)
            .get_result(conn)
            .await?;
        Ok(deployment)
    }
}
