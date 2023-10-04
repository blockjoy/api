//! The metrics service is the service that relates to the metrics for nodes and hosts that we
//! gather. At some point we may switch to a provisioned metrics service, so for now this service
//! does not store a history of metrics. Rather, it overwrites the metrics that are know for each
//! time new ones are provided. This makes sure that the database doesn't grow overly large.

use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use itertools::Itertools;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::MetricsPerm;
use crate::auth::resource::NodeId;
use crate::auth::Authorize;
use crate::database::{Transaction, WriteConn};
use crate::models::host::UpdateHostMetrics;
use crate::models::node::{NodeJob, UpdateNodeMetrics};
use crate::models::{Host, Node};

use super::api::metrics_service_server::MetricsService;
use super::{api, Grpc};

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
    Host(#[from] crate::models::host::Error),
    /// Metrics MQTT message error: {0}
    Message(Box<crate::mqtt::message::Error>),
    /// Failed to parse network received: {0}
    NetworkReceived(std::num::TryFromIntError),
    /// Failed to parse network sent: {0}
    NetworkSent(std::num::TryFromIntError),
    /// Failed to parse HostId: {0}
    ParseHostId(uuid::Error),
    /// Failed to parse NodeId: {0}
    ParseNodeId(uuid::Error),
    /// Metrics node error: {0}
    Node(#[from] crate::models::node::Error),
    /// Failed to parse current data sync progress: {0}
    SyncCurrent(std::num::TryFromIntError),
    /// Failed to parse total data sync progress: {0}
    SyncTotal(std::num::TryFromIntError),
    /// Failed to parse uptime: {0}
    Uptime(std::num::TryFromIntError),
    /// Failed to parse used cpu: {0}
    UsedCpu(std::num::TryFromIntError),
    /// Failed to parse used disk space: {0}
    UsedDisk(std::num::TryFromIntError),
    /// Failed to parse used memory: {0}
    UsedMemory(std::num::TryFromIntError),
    /// Attempt to update the metrics for node(s) `{msg}`, which don't exist
    MetricsForMissingNode { msg: String },
    /// Attempt to update the metrics for host(s) `{msg}`, which don't exist
    MetricsForMissingHost { msg: String },
    /// Could not serialize jobs: {0}
    UnserializableJobs(serde_json::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | Message(_) | UnserializableJobs(_) => Status::internal("Internal error."),
            BlockAge(_) => Status::invalid_argument("block_age"),
            BlockHeight(_) => Status::invalid_argument("height"),
            NetworkReceived(_) => Status::invalid_argument("network_received"),
            NetworkSent(_) => Status::invalid_argument("network_sent"),
            ParseNodeId(_) => Status::invalid_argument("metrics.id"),
            ParseHostId(_) => Status::invalid_argument("metrics.id"),
            SyncCurrent(_) => Status::invalid_argument("data_sync_progress_current"),
            SyncTotal(_) => Status::invalid_argument("data_sync_progress_total"),
            Uptime(_) => Status::invalid_argument("uptime"),
            UsedCpu(_) => Status::invalid_argument("used_cpu"),
            UsedDisk(_) => Status::invalid_argument("used_disk_space"),
            UsedMemory(_) => Status::invalid_argument("used_memory"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Host(err) => err.into(),
            Node(err) => err.into(),
            MetricsForMissingNode { .. } => Status::not_found("Not found."),
            MetricsForMissingHost { .. } => Status::not_found("Not found."),
        }
    }
}

#[tonic::async_trait]
impl MetricsService for Grpc {
    /// Update the metrics for the nodes provided in this request. Since this endpoint is called
    /// often (e.g. if we have 10,000 nodes, 170 calls per second) we take care to perform a single
    /// query for this whole list of metrics that comes in.
    async fn node(
        &self,
        req: Request<api::MetricsServiceNodeRequest>,
    ) -> Result<Response<api::MetricsServiceNodeResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        let outcome = self
            .write(|write| node(req, meta, write).scope_boxed())
            .await?;
        match outcome.into_inner() {
            RespOrError::Resp(resp) => Ok(tonic::Response::new(resp)),
            RespOrError::Error(error) => Err(error.into()),
        }
    }

    async fn host(
        &self,
        req: Request<api::MetricsServiceHostRequest>,
    ) -> Result<Response<api::MetricsServiceHostResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        let outcome = self
            .write(|write| host(req, meta, write).scope_boxed())
            .await?;
        match outcome.into_inner() {
            RespOrError::Resp(resp) => Ok(tonic::Response::new(resp)),
            RespOrError::Error(error) => Err(error.into()),
        }
    }
}

enum RespOrError<T> {
    Resp(T),
    Error(Error),
}

async fn node(
    req: api::MetricsServiceNodeRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<RespOrError<api::MetricsServiceNodeResponse>, Error> {
    // First we split our map of `node_id`: `update info` into two vectors, so we can parse and
    // validate all node ids. We use vectors so we can preserve ordering and later use `zip` to
    // match them together again.
    let (all_node_ids, updates): (Vec<_>, Vec<_>) = req.metrics.into_iter().unzip();
    // Parse all node_ids and error out if there are any issues.
    let all_node_ids: Vec<_> = all_node_ids
        .into_iter()
        .map(|id| id.parse().map_err(Error::ParseNodeId))
        .collect::<Result<_, Error>>()?;
    // Now we find the list of nodes that actually exist in the database. If there are any missing
    // node ids, we continue on with our update, since this is a common error case that needs to be
    // handled by performing the update for all nodes that do exist, and then reporting any issues.
    let node_ids = Node::existing_ids(all_node_ids.iter().copied().collect(), &mut write).await?;
    // Check that the user has metrics-access to all nodes that do exist.
    write.auth(&meta, MetricsPerm::Node, &node_ids).await?;
    // Query all the nodes from the database. We need the info from the `node.jobs` field to perform
    // a patch-sort-of-update on that field.
    let nodes = Node::find_by_ids(node_ids.clone(), &mut write).await?;
    // Now we can create the UpdateNodeMetrics models using our existing, queried nodes.
    let nodes_map: HashMap<NodeId, &Node> = nodes.iter().map(|n| (n.id, n)).collect();
    let updates = updates
        .into_iter()
        .zip(all_node_ids.iter())
        .filter_map(|(update, id)| nodes_map.get(id).map(|&node| (node, update)))
        .map(|(node, update)| update.as_metrics_update(node))
        .collect::<Result<_, _>>()?;
    let nodes = UpdateNodeMetrics::update_metrics(updates, &mut write).await?;
    api::NodeMessage::updated_many(nodes, &mut write)
        .await
        .map_err(|err| Error::Message(Box::new(err)))?
        .into_iter()
        .for_each(|msg| write.mqtt(msg));
    // We find the difference between the user-provided node ids and the ones that actually exist.
    // If there is such a difference, we use it to provide a nice error message to the user.
    let missing: Vec<NodeId> = all_node_ids
        .into_iter()
        .filter(|id| !node_ids.contains(id))
        .collect();
    if missing.is_empty() {
        Ok(RespOrError::Resp(api::MetricsServiceNodeResponse {}))
    } else {
        let msg = missing.iter().join(", ");
        Ok(RespOrError::Error(Error::MetricsForMissingNode { msg }))
    }
}

async fn host(
    req: api::MetricsServiceHostRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<RespOrError<api::MetricsServiceHostResponse>, Error> {
    let updates = req
        .metrics
        .into_iter()
        .map(|(key, val)| val.as_metrics_update(&key))
        .collect::<Result<Vec<_>, _>>()?;

    let host_ids = updates.iter().map(|update| update.id).collect();
    let host_ids = Host::existing_ids(host_ids, &mut write).await?;
    write.auth(&meta, MetricsPerm::Host, &host_ids).await?;

    let (updates, missing) = updates.into_iter().partition(|u| host_ids.contains(&u.id));
    let hosts = UpdateHostMetrics::update_metrics(updates, &mut write).await?;

    api::HostMessage::updated_many(hosts, &mut write)
        .await
        .map_err(|err| Error::Message(Box::new(err)))?
        .into_iter()
        .for_each(|msg| write.mqtt(msg));

    if missing.is_empty() {
        Ok(RespOrError::Resp(api::MetricsServiceHostResponse {}))
    } else {
        let msg = missing.iter().map(|m| m.id).join(", ");
        Ok(RespOrError::Error(Error::MetricsForMissingHost { msg }))
    }
}

impl api::NodeMetrics {
    pub fn as_metrics_update(self, node: &Node) -> Result<UpdateNodeMetrics, Error> {
        let jobs = self.merge_jobs(node)?;
        Ok(UpdateNodeMetrics {
            id: node.id,
            block_height: self
                .height
                .map(i64::try_from)
                .transpose()
                .map_err(Error::BlockHeight)?,
            block_age: self
                .block_age
                .map(i64::try_from)
                .transpose()
                .map_err(Error::BlockAge)?,
            staking_status: Some(self.staking_status().into_model()),
            consensus: self.consensus,
            chain_status: Some(self.application_status().into_model()),
            sync_status: Some(self.sync_status().into_model()),
            jobs: jobs
                .map(serde_json::to_value)
                .transpose()
                .map_err(Error::UnserializableJobs)?,
        })
    }

    /// Merge the jobs in `self.jobs` with `node.jobs`, overwriting jobs in `node.jobs` with any
    /// jobs from `self.jobs` if they have the same name. We only need to perform an update if
    /// `self.jobs` contains data, so this method returns Ok(None) in that case.
    fn merge_jobs(&self, node: &Node) -> Result<Option<Vec<NodeJob>>, Error> {
        if self.jobs.is_empty() {
            return Ok(None);
        }
        let jobs: HashMap<String, NodeJob> = node
            .jobs()?
            .into_iter()
            .chain(self.jobs.iter().cloned().map(api::NodeJob::into_model))
            .map(|n| (n.name.clone(), n))
            .collect();
        Ok(Some(jobs.into_values().collect()))
    }
}

impl api::HostMetrics {
    pub fn as_metrics_update(self, host_id: &str) -> Result<UpdateHostMetrics, Error> {
        Ok(UpdateHostMetrics {
            id: host_id.parse().map_err(Error::ParseHostId)?,
            used_cpu: self
                .used_cpu
                .map(i32::try_from)
                .transpose()
                .map_err(Error::UsedCpu)?,
            used_memory: self
                .used_memory
                .map(i64::try_from)
                .transpose()
                .map_err(Error::UsedMemory)?,
            used_disk_space: self
                .used_disk_space
                .map(i64::try_from)
                .transpose()
                .map_err(Error::UsedDisk)?,
            load_one: self.load_one,
            load_five: self.load_five,
            load_fifteen: self.load_fifteen,
            network_received: self
                .network_received
                .map(i64::try_from)
                .transpose()
                .map_err(Error::NetworkReceived)?,
            network_sent: self
                .network_sent
                .map(i64::try_from)
                .transpose()
                .map_err(Error::NetworkSent)?,
            uptime: self
                .uptime
                .map(i64::try_from)
                .transpose()
                .map_err(Error::Uptime)?,
        })
    }
}
