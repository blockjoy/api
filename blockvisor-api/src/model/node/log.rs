use std::collections::HashMap;

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use displaydoc::Display;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::{HostId, NodeId, OrgId};
use crate::database::Conn;
use crate::grpc::Status;
use crate::model::schema::{node_logs, sql_types};
use crate::model::{BlockchainId, Host};

use super::{NodeType, NodeVersion};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create new node log: {0}
    Create(diesel::result::Error),
    /// Failed to find node log by node: {0}
    ByNode(diesel::result::Error),
    /// Failed to find recent node logs by node: {0}
    ByNodeSince(diesel::result::Error),
    /// Node log host error: {0}
    Host(#[from] crate::model::host::Error),
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
    CreateSucceeded,
    /// This variant is used to note that a node was not created. When we receive this variant, we
    /// will send a `NodeDelete` message to blockvisord to clean up, and this message should either
    /// be followed by a `Created` or a `Canceled` log entry, depending on whether we dediced to
    /// retry or to abort.
    CreateFailed,
    /// This variant is used to note that we aborted from creating the node, because the failure we
    /// ran into was endemic.
    Canceled,
    /// This node was transferred to another org.
    TransferredToOrg,
    /// Log that an `UpgradeNode` message has been sent to blockvisord. This
    /// should be followed by `UpgradeSucceeded` or `UpgradeFailed` afterwards.
    Upgraded,
    /// Confirmation that a node was successfully upgraded.
    UpgradeSucceeded,
    /// Notification that an attempt to upgrade a node failed.
    UpgradeFailed,
}

/// Records to indicate that some node deployment event has happened.
#[derive(Debug, Queryable)]
pub struct NodeLog {
    pub id: Uuid,
    pub host_id: HostId,
    pub node_id: NodeId,
    pub event: NodeLogEvent,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub node_type: NodeType,
    pub org_id: OrgId,
    pub blockchain_id: BlockchainId,
}

impl NodeLog {
    pub async fn by_node_id(node_id: NodeId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        node_logs::table
            .filter(node_logs::node_id.eq(node_id))
            .get_results(conn)
            .await
            .map_err(Error::ByNode)
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
        let hosts = Host::by_ids(host_ids, conn).await?;
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
pub struct NewNodeLog {
    pub host_id: HostId,
    pub node_id: NodeId,
    pub event: NodeLogEvent,
    pub version: NodeVersion,
    pub node_type: NodeType,
    pub created_at: DateTime<Utc>,
    pub org_id: OrgId,
    pub blockchain_id: BlockchainId,
}

impl NewNodeLog {
    pub async fn create(self, conn: &mut Conn<'_>) -> Result<NodeLog, Error> {
        diesel::insert_into(node_logs::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}
