//! The metrics service is the service that relates to the metrics for nodes and hosts that we
//! gather. At some point we may switch to a provisioned metrics service, so for now this service
//! does not store a history of metrics. Rather, it overwrites the metrics that are know for each
//! time new ones are provided. This makes sure that the database doesn't grow overly large.

use super::api::{self, metrics_server};
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;

#[tonic::async_trait]
impl metrics_server::Metrics for super::GrpcImpl {
    /// Update the metrics for the nodes provided in this request. Since this endpoint is called
    /// often (e.g. if we have 10,000 nodes, 170 calls per second) we take care to perform a single
    /// query for this whole list of metrics that comes in.
    async fn node(&self, request: tonic::Request<api::NodeMetricsRequest>) -> super::Result<()> {
        let request = request.into_inner();
        let updates = request
            .metrics
            .into_iter()
            .map(|(k, v)| v.as_metrics_update(&k))
            .collect::<crate::Result<_>>()?;
        self.trx(|c| models::UpdateNodeMetrics::update_metrics(updates, c).scope_boxed())
            .await?;
        Ok(tonic::Response::new(()))
    }

    async fn host(&self, request: tonic::Request<api::HostMetricsRequest>) -> super::Result<()> {
        let request = request.into_inner();
        let updates = request
            .metrics
            .into_iter()
            .map(|(k, v)| v.as_metrics_update(&k))
            .collect::<crate::Result<_>>()?;
        self.trx(|c| models::UpdateHostMetrics::update_metrics(updates, c).scope_boxed())
            .await?;
        Ok(tonic::Response::new(()))
    }
}

impl api::NodeMetrics {
    pub fn as_metrics_update(self, id: &str) -> crate::Result<models::UpdateNodeMetrics> {
        let id = id.parse()?;
        Ok(models::UpdateNodeMetrics {
            id,
            block_height: self.height.map(i64::try_from).transpose()?,
            block_age: self.block_age.map(i64::try_from).transpose()?,
            staking_status: self
                .staking_status
                .map(models::NodeStakingStatus::try_from)
                .transpose()?,
            consensus: self.consensus,
            chain_status: self.application_status.map(TryInto::try_into).transpose()?,
            sync_status: self.sync_status.map(TryInto::try_into).transpose()?,
        })
    }
}

impl api::HostMetrics {
    pub fn as_metrics_update(self, id: &str) -> crate::Result<models::UpdateHostMetrics> {
        let id = id.parse()?;
        Ok(models::UpdateHostMetrics {
            id,
            used_cpu: self.used_cpu.map(i32::try_from).transpose()?,
            used_memory: self.used_memory.map(i64::try_from).transpose()?,
            used_disk_space: self.used_disk_space.map(i64::try_from).transpose()?,
            load_one: self.load_one,
            load_five: self.load_five,
            load_fifteen: self.load_fifteen,
            network_received: self.network_received.map(i64::try_from).transpose()?,
            network_sent: self.network_sent.map(i64::try_from).transpose()?,
            uptime: self.uptime.map(i64::try_from).transpose()?,
        })
    }
}
