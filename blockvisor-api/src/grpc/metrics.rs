//! The metrics service handles metrics updates for hosts and nodes.

use std::collections::HashSet;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use itertools::Itertools;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::MetricsPerm;
use crate::auth::resource::{HostId, NodeId, Resource};
use crate::auth::Authorize;
use crate::database::{Transaction, WriteConn};
use crate::model::host::{Host, UpdateHostMetrics};
use crate::model::node::{Node, NodeJobs, NodeStatus, UpdateNodeMetrics};
use crate::util::HashVec;

use super::api::metrics_service_server::MetricsService;
use super::{api, common, Grpc, Metadata, Status};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Failed to parse block age: {0}
    BlockAge(std::num::TryFromIntError),
    /// Failed to parse block height: {0}
    BlockHeight(std::num::TryFromIntError),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Metrics host error: {0}
    Host(#[from] crate::model::host::Error),
    /// Metrics host grpc error: {0}
    HostGrpc(#[from] crate::grpc::host::Error),
    /// Attempt to update the metrics for node `{node_id}`, which doesn't exist HostId: {host_id:?}
    MetricsForMissingNode {
        node_id: NodeId,
        host_id: Option<HostId>,
    },
    /// Attempt to update the metrics for nodes `{msg}`, which don't exist. HostId: {host_id:?}
    MetricsForMissingNodes {
        msg: String,
        host_id: Option<HostId>,
    },
    /// Missing HostMetrics.
    MissingHostMetrics,
    /// Failed to parse network received: {0}
    NetworkReceived(std::num::TryFromIntError),
    /// Failed to parse network sent: {0}
    NetworkSent(std::num::TryFromIntError),
    /// Node metrics error: {0}
    Node(#[from] crate::model::node::Error),
    /// Grpc node metrics error: {0}
    NodeGrpc(#[from] crate::grpc::node::Error),
    /// Node status metrics error: {0}
    NodeStatus(#[from] crate::model::node::status::Error),
    /// Host token required for updating public hosts.
    NotHostToken,
    /// Failed to parse HostId: {0}
    ParseHostId(uuid::Error),
    /// Failed to parse NodeId: {0}
    ParseNodeId(uuid::Error),
    /// Metrics resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// Failed to parse current data sync progress: {0}
    SyncCurrent(std::num::TryFromIntError),
    /// Failed to parse total data sync progress: {0}
    SyncTotal(std::num::TryFromIntError),
    /// Failed to parse uptime: {0}
    Uptime(std::num::TryFromIntError),
    /// Failed to parse used cpu hundreths: {0}
    UsedCpu(std::num::TryFromIntError),
    /// Failed to parse used disk space: {0}
    UsedDisk(std::num::TryFromIntError),
    /// Failed to parse used memory: {0}
    UsedMemory(std::num::TryFromIntError),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) => Status::internal("Internal error."),
            BlockAge(_) => Status::invalid_argument("block_age"),
            BlockHeight(_) => Status::invalid_argument("height"),
            MetricsForMissingNode { .. } => Status::not_found("Not found."),
            MetricsForMissingNodes { .. } => Status::not_found("Not found."),
            MissingHostMetrics => Status::invalid_argument("metrics"),
            NetworkReceived(_) => Status::invalid_argument("network_received"),
            NetworkSent(_) => Status::invalid_argument("network_sent"),
            NotHostToken => Status::forbidden("Access denied."),
            ParseHostId(_) => Status::invalid_argument("metrics.host_id"),
            ParseNodeId(_) => Status::invalid_argument("metrics.node_id"),
            SyncCurrent(_) => Status::invalid_argument("data_sync_progress_current"),
            SyncTotal(_) => Status::invalid_argument("data_sync_progress_total"),
            Uptime(_) => Status::invalid_argument("uptime_seconds"),
            UsedCpu(_) => Status::invalid_argument("used_cpu_hundreths"),
            UsedDisk(_) => Status::invalid_argument("used_disk_bytes"),
            UsedMemory(_) => Status::invalid_argument("used_memory_bytes"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Host(err) => err.into(),
            HostGrpc(err) => err.into(),
            Node(err) => err.into(),
            NodeGrpc(err) => err.into(),
            NodeStatus(err) => err.into(),
            Resource(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl MetricsService for Grpc {
    async fn host(
        &self,
        req: Request<api::MetricsServiceHostRequest>,
    ) -> Result<Response<api::MetricsServiceHostResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| host(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn node(
        &self,
        req: Request<api::MetricsServiceNodeRequest>,
    ) -> Result<Response<api::MetricsServiceNodeResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        let outcome: Result<Response<AfterCommit<api::MetricsServiceNodeResponse>>, tonic::Status> =
            self.write(|write| node(req, meta.into(), write).scope_boxed())
                .await;
        match outcome?.into_inner() {
            AfterCommit::Ok(resp) => Ok(Response::new(resp)),
            AfterCommit::Err(err) => Err(Status::from(err).into()),
        }
    }
}

pub async fn host(
    req: api::MetricsServiceHostRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::MetricsServiceHostResponse, Error> {
    let update = req
        .metrics
        .ok_or(Error::MissingHostMetrics)?
        .into_update()?;

    let authz = write.auth_for(&meta, MetricsPerm::Host, update.id).await?;
    let org_id = Host::org_id(update.id, &mut write).await?;
    if org_id.is_none() && authz.resource().host().is_none() {
        return Err(Error::NotHostToken);
    }

    let host = UpdateHostMetrics::apply(&update, &mut write).await?;
    let host = api::Host::from_host(host, &mut write).await?;

    let updated_by = common::Resource::from(&authz);
    let host_msg = api::HostMessage::updated(host, updated_by);
    write.mqtt(host_msg);

    Ok(api::MetricsServiceHostResponse {})
}

pub async fn node(
    req: api::MetricsServiceNodeRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<AfterCommit<api::MetricsServiceNodeResponse>, Error> {
    let updates = req
        .metrics
        .into_iter()
        .map(api::NodeMetrics::into_update)
        .collect::<Result<Vec<_>, _>>()?;

    let update_ids: HashSet<_> = updates.iter().map(|update| update.id).collect();
    let nodes = Node::by_ids(&update_ids, &mut write).await?;

    let node_ids: HashSet<_> = nodes.iter().map(|node| node.id).collect();
    let resources: Vec<_> = node_ids.iter().map(|id| Resource::from(*id)).collect();
    let authz = write.auth_for(&meta, MetricsPerm::Node, &resources).await?;

    let nodes_map = nodes.iter().to_map_keep_last(|node| (node.id, node));

    let nodes = UpdateNodeMetrics::apply_all(updates, &mut write).await?;
    let nodes = api::Node::from_models(nodes, &authz, &mut write).await?;

    let updated_by = common::Resource::from(&authz);
    api::NodeMessage::updated_many(nodes, &updated_by)
        .into_iter()
        .for_each(|msg| write.mqtt(msg));

    let host_id = authz.claims.resource().host();
    let missing: Vec<NodeId> = update_ids
        .into_iter()
        .filter(|id| !nodes_map.contains_key(id))
        .collect();
    match missing.as_slice() {
        [] => Ok(AfterCommit::Ok(api::MetricsServiceNodeResponse {})),
        &[node_id] => Ok(AfterCommit::Err(Error::MetricsForMissingNode {
            node_id,
            host_id,
        })),
        _ => {
            let msg = missing.iter().join(", ");
            Ok(AfterCommit::Err(Error::MetricsForMissingNodes {
                msg,
                host_id,
            }))
        }
    }
}

/// The response to send over gRPC after committing the transaction.
pub enum AfterCommit<T> {
    Ok(T),
    Err(Error),
}

impl api::HostMetrics {
    pub fn into_update(self) -> Result<UpdateHostMetrics, Error> {
        Ok(UpdateHostMetrics {
            id: self.host_id.parse().map_err(Error::ParseHostId)?,
            used_cpu_hundreths: self
                .used_cpu_hundreths
                .map(|cpu| i64::try_from(cpu).map_err(Error::UsedCpu))
                .transpose()?,
            used_memory_bytes: self
                .used_memory_bytes
                .map(|memory| i64::try_from(memory).map_err(Error::UsedMemory))
                .transpose()?,
            used_disk_bytes: self
                .used_disk_bytes
                .map(|disk| i64::try_from(disk).map_err(Error::UsedDisk))
                .transpose()?,
            load_one_percent: self.load_one_percent,
            load_five_percent: self.load_five_percent,
            load_fifteen_percent: self.load_fifteen_percent,
            network_received_bytes: self
                .network_received_bytes
                .map(i64::try_from)
                .transpose()
                .map_err(Error::NetworkReceived)?,
            network_sent_bytes: self
                .network_sent_bytes
                .map(i64::try_from)
                .transpose()
                .map_err(Error::NetworkSent)?,
            uptime_seconds: self
                .uptime_seconds
                .map(i64::try_from)
                .transpose()
                .map_err(Error::Uptime)?,
        })
    }
}

impl api::NodeMetrics {
    pub fn into_update(self) -> Result<UpdateNodeMetrics, Error> {
        let id = self.node_id.parse().map_err(Error::ParseNodeId)?;

        let node_status = self.node_status.map(NodeStatus::try_from).transpose()?;
        let node_state = node_status.as_ref().map(|status| status.state);
        let protocol_health = node_status
            .as_ref()
            .and_then(|status| status.protocol.as_ref())
            .map(|proto| proto.health);
        let protocol_state = node_status
            .and_then(|status| status.protocol)
            .map(|proto| proto.state);

        let block_height = self
            .height
            .map(i64::try_from)
            .transpose()
            .map_err(Error::BlockHeight)?;
        let block_age = self
            .block_age
            .map(i64::try_from)
            .transpose()
            .map_err(Error::BlockAge)?;
        let jobs: NodeJobs = self
            .jobs
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>()
            .into();

        Ok(UpdateNodeMetrics {
            id,
            node_state,
            protocol_state,
            protocol_health,
            block_height,
            block_age,
            consensus: self.consensus,
            jobs: Some(jobs),
        })
    }
}
