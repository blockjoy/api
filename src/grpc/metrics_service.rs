//! The metrics service is the service that relates to the metrics for nodes and hosts that we
//! gather. At some point we may switch to a provisioned metrics service, so for now this service
//! does not store a history of metrics. Rather, it overwrites the metrics that are know for each
//! time new ones are provided. This makes sure that the database doesn't grow overly large.

use crate::grpc::blockjoy::{self, metrics_service_server::MetricsService};
use crate::models;
use tonic::Response;

pub struct MetricsServiceImpl {
    db: models::DbPool,
}

impl MetricsServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl MetricsService for MetricsServiceImpl {
    /// Update the metrics for the nodes provided in this request. Since this endpoint is called
    /// often (e.g. if we have 10,000 nodes, 170 calls per second) we take care to perform a single
    /// query for this whole list of metrics that comes in.
    async fn node(
        &self,
        request: tonic::Request<blockjoy::NodeMetricsRequest>,
    ) -> Result<Response<()>, tonic::Status> {
        let request = request.into_inner();
        let updates = request
            .metrics
            .into_iter()
            .map(|(k, v)| models::NodeMetricsUpdate::from_metrics(k, v))
            .collect::<Result<_, _>>()?;
        let mut tx = self.db.begin().await?;
        models::NodeMetricsUpdate::update_metrics(updates, &mut tx).await?;
        tx.commit().await?;
        Ok(tonic::Response::new(()))
    }

    async fn host(
        &self,
        request: tonic::Request<blockjoy::HostMetricsRequest>,
    ) -> Result<Response<()>, tonic::Status> {
        let request = request.into_inner();
        let updates = request
            .metrics
            .into_iter()
            .map(|(k, v)| models::HostSelectiveUpdate::from_metrics(k, v))
            .collect::<Result<_, _>>()?;
        let mut tx = self.db.begin().await?;
        models::HostSelectiveUpdate::update_metrics(updates, &mut tx).await?;
        tx.commit().await?;
        Ok(tonic::Response::new(()))
    }
}
