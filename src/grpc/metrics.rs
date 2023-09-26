//! The metrics service is the service that relates to the metrics for nodes and hosts that we
//! gather. At some point we may switch to a provisioned metrics service, so for now this service
//! does not store a history of metrics. Rather, it overwrites the metrics that are know for each
//! time new ones are provided. This makes sure that the database doesn't grow overly large.

use std::collections::HashSet;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::MetricsPerm;
use crate::auth::Authorize;
use crate::database::{Transaction, WriteConn};
use crate::models::host::UpdateHostMetrics;
use crate::models::node::UpdateNodeMetrics;

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
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!("{err}");
        use Error::*;
        match err {
            Diesel(_) | Message(_) => Status::internal("Internal error."),
            BlockAge(_) => Status::invalid_argument("block_age"),
            BlockHeight(_) => Status::invalid_argument("height"),
            NetworkReceived(_) => Status::invalid_argument("network_received"),
            NetworkSent(_) => Status::invalid_argument("network_sent"),
            ParseHostId(_) => Status::invalid_argument("metrics.id"),
            ParseNodeId(_) => Status::invalid_argument("metrics.id"),
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
            RespOrErrors::Resp(resp) => Ok(tonic::Response::new(resp)),
            RespOrErrors::Errors(errors) => Err(summarize(errors.into_iter())),
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
            RespOrErrors::Resp(resp) => Ok(tonic::Response::new(resp)),
            RespOrErrors::Errors(errors) => Err(summarize(errors.into_iter())),
        }
    }
}

enum RespOrErrors<T> {
    Resp(T),
    Errors(Vec<Error>),
}

async fn node(
    req: api::MetricsServiceNodeRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<RespOrErrors<api::MetricsServiceNodeResponse>, Error> {
    let updates = req
        .metrics
        .into_iter()
        .map(|(key, val)| val.as_metrics_update(&key))
        .collect::<Result<Vec<_>, _>>()?;

    let node_ids: HashSet<_> = updates.iter().map(|update| update.id).collect();
    let _ = write.auth(&meta, MetricsPerm::Node, &node_ids).await?;

    let (nodes, errors) = UpdateNodeMetrics::update_metrics(updates, &mut write).await;

    api::NodeMessage::updated_many(nodes, &mut write)
        .await
        .map_err(|err| Error::Message(Box::new(err)))?
        .into_iter()
        .for_each(|msg| write.mqtt(msg));

    match errors.len() {
        0 => Ok(RespOrErrors::Resp(api::MetricsServiceNodeResponse {})),
        _ => Ok(RespOrErrors::Errors(
            errors.into_iter().map(Error::Node).collect(),
        )),
    }
}

async fn host(
    req: api::MetricsServiceHostRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<RespOrErrors<api::MetricsServiceHostResponse>, Error> {
    let updates = req
        .metrics
        .into_iter()
        .map(|(key, val)| val.as_metrics_update(&key))
        .collect::<Result<Vec<_>, _>>()?;

    let host_ids: HashSet<_> = updates.iter().map(|update| update.id).collect();
    let _ = write.auth(&meta, MetricsPerm::Host, &host_ids).await?;

    let (hosts, errors) = UpdateHostMetrics::update_metrics(updates, &mut write).await;

    api::HostMessage::updated_many(hosts, &mut write)
        .await
        .map_err(|err| Error::Message(Box::new(err)))?
        .into_iter()
        .for_each(|msg| write.mqtt(msg));

    match errors.len() {
        0 => Ok(RespOrErrors::Resp(api::MetricsServiceHostResponse {})),
        _ => Ok(RespOrErrors::Errors(
            errors.into_iter().map(Error::Host).collect(),
        )),
    }
}

impl api::NodeMetrics {
    pub fn as_metrics_update(self, node_id: &str) -> Result<UpdateNodeMetrics, Error> {
        Ok(UpdateNodeMetrics {
            id: node_id.parse().map_err(Error::ParseNodeId)?,
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
            data_sync_progress_total: self
                .data_sync_progress_total
                .map(i32::try_from)
                .transpose()
                .map_err(Error::SyncTotal)?,
            data_sync_progress_current: self
                .data_sync_progress_current
                .map(i32::try_from)
                .transpose()
                .map_err(Error::SyncCurrent)?,
            data_sync_progress_message: self.data_sync_progress_message,
        })
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

fn summarize(errors: impl Iterator<Item = Error>) -> tonic::Status {
    let combine = |s1, s2| s1 + format!("{s2:?}").as_str() + ",";
    let msg = errors.fold(String::new(), combine);
    tonic::Status::internal(msg)
}
