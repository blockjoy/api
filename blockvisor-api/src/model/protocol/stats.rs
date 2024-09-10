use std::collections::HashMap;

use diesel::dsl::count;
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;

use crate::auth::rbac::ProtocolAdminPerm;
use crate::auth::AuthZ;
use crate::database::Conn;
use crate::grpc::{api, Status};
use crate::model::node::NodeState;
use crate::model::schema::nodes;

use super::{Protocol, ProtocolId, ProtocolVersion, VersionId};

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to get stats for all protocols: {0}
    ForAllProtocols(diesel::result::Error),
    /// Failed to get stats for all versions: {0}
    ForAllVersions(diesel::result::Error),
    /// Failed to get stats for protocol id `{0}`: {1}
    ForProtocol(ProtocolId, diesel::result::Error),
    /// Failed to get stats for protocol version id `{0}`: {1}
    ForVersion(VersionId, diesel::result::Error),
    /// Missing the permission to view all protocol stats.
    MissingViewAll,
    /// Unable to cast failed node count from i64 to u64: {0}
    NodeFailed(std::num::TryFromIntError),
    /// Unable to cast node running from i64 to u64: {0}
    NodeRunning(std::num::TryFromIntError),
    /// Unable to cast node starting from i64 to u64: {0}
    NodeStarting(std::num::TryFromIntError),
    /// Unable to cast node total from i64 to u64: {0}
    NodeTotal(std::num::TryFromIntError),
    /// Unable to cast node upgrading from i64 to u64: {0}
    NodeUpgrading(std::num::TryFromIntError),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ForProtocol(_, NotFound) | ForVersion(_, NotFound) => Status::not_found("Not found."),
            MissingViewAll => Status::forbidden("Access denied."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Queryable)]
pub struct NodeStats {
    pub total: i64,
    pub starting: i64,
    pub running: i64,
    pub upgrading: i64,
    pub failed: i64,
}

impl NodeStats {
    pub async fn for_protocol(
        protocol: &Protocol,
        conn: &mut Conn<'_>,
    ) -> Result<NodeStats, Error> {
        nodes::table
            .filter(nodes::protocol_id.eq(protocol.id))
            .filter(nodes::deleted_at.is_null())
            .select((
                count(nodes::id),
                count(nodes::node_state.eq(NodeState::Starting)),
                count(nodes::node_state.eq(NodeState::Running)),
                count(nodes::node_state.eq(NodeState::Upgrading)),
                count(nodes::node_state.eq(NodeState::Failed)),
            ))
            .get_result(conn)
            .await
            .map_err(|err| Error::ForProtocol(protocol.id, err))
    }

    pub async fn for_version(
        version: &ProtocolVersion,
        conn: &mut Conn<'_>,
    ) -> Result<NodeStats, Error> {
        nodes::table
            .filter(nodes::protocol_version_id.eq(version.id))
            .filter(nodes::deleted_at.is_null())
            .select((
                count(nodes::id),
                count(nodes::node_state.eq(NodeState::Starting)),
                count(nodes::node_state.eq(NodeState::Running)),
                count(nodes::node_state.eq(NodeState::Upgrading)),
                count(nodes::node_state.eq(NodeState::Failed)),
            ))
            .get_result(conn)
            .await
            .map_err(|err| Error::ForVersion(version.id, err))
    }

    pub async fn for_all_protocols(
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<HashMap<ProtocolId, NodeStats>, Error> {
        if !authz.has_perm(ProtocolAdminPerm::ViewAllStats) {
            return Err(Error::MissingViewAll);
        }

        let stats: Vec<(ProtocolId, i64, i64, i64, i64, i64)> = nodes::table
            .filter(nodes::deleted_at.is_null())
            .group_by(nodes::protocol_id)
            .select((
                nodes::protocol_id,
                count(nodes::id),
                count(nodes::node_state.eq(NodeState::Starting)),
                count(nodes::node_state.eq(NodeState::Running)),
                count(nodes::node_state.eq(NodeState::Upgrading)),
                count(nodes::node_state.eq(NodeState::Failed)),
            ))
            .get_results(conn)
            .await
            .map_err(Error::ForAllProtocols)?;

        #[rustfmt::skip]
        let stats_map = stats
            .into_iter()
            .map(|(protocol_id, total, starting, running, upgrading, failed)| {
                (protocol_id, NodeStats { total, starting, running, upgrading, failed })
            })
            .collect::<HashMap<ProtocolId, NodeStats>>();

        Ok(stats_map)
    }

    pub async fn for_all_versions(
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<HashMap<VersionId, NodeStats>, Error> {
        if !authz.has_perm(ProtocolAdminPerm::ViewAllStats) {
            return Err(Error::MissingViewAll);
        }

        let stats: Vec<(VersionId, i64, i64, i64, i64, i64)> = nodes::table
            .filter(nodes::deleted_at.is_null())
            .group_by(nodes::protocol_version_id)
            .select((
                nodes::protocol_version_id,
                count(nodes::id),
                count(nodes::node_state.eq(NodeState::Starting)),
                count(nodes::node_state.eq(NodeState::Running)),
                count(nodes::node_state.eq(NodeState::Upgrading)),
                count(nodes::node_state.eq(NodeState::Failed)),
            ))
            .get_results(conn)
            .await
            .map_err(Error::ForAllVersions)?;

        #[rustfmt::skip]
        let stats_map = stats
            .into_iter()
            .map(|(version_id, total, starting, running, upgrading, failed)| {
                (version_id, NodeStats { total, starting, running, upgrading, failed })
            })
            .collect::<HashMap<VersionId, NodeStats>>();

        Ok(stats_map)
    }
}

impl TryFrom<NodeStats> for api::NodeStats {
    type Error = Error;

    fn try_from(stats: NodeStats) -> Result<Self, Self::Error> {
        Ok(api::NodeStats {
            total: stats.total.try_into().map_err(Error::NodeTotal)?,
            starting: stats.starting.try_into().map_err(Error::NodeStarting)?,
            running: stats.running.try_into().map_err(Error::NodeRunning)?,
            upgrading: stats.upgrading.try_into().map_err(Error::NodeUpgrading)?,
            failed: stats.failed.try_into().map_err(Error::NodeFailed)?,
        })
    }
}
