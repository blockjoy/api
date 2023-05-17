//! The metrics service is the service that relates to the metrics for nodes and hosts that we
//! gather. At some point we may switch to a provisioned metrics service, so for now this service
//! does not store a history of metrics. Rather, it overwrites the metrics that are know for each
//! time new ones are provided. This makes sure that the database doesn't grow overly large.

use super::api::{self, metrics_service_server};
use crate::{auth, models};
use diesel_async::scoped_futures::ScopedFutureExt;
use std::collections::HashSet;

#[tonic::async_trait]
impl metrics_service_server::MetricsService for super::GrpcImpl {
    /// Update the metrics for the nodes provided in this request. Since this endpoint is called
    /// often (e.g. if we have 10,000 nodes, 170 calls per second) we take care to perform a single
    /// query for this whole list of metrics that comes in.
    async fn node(
        &self,
        req: tonic::Request<api::MetricsServiceNodeRequest>,
    ) -> super::Resp<api::MetricsServiceNodeResponse> {
        self.trx(|c| node(req, c).scope_boxed()).await
    }

    async fn host(
        &self,
        req: tonic::Request<api::MetricsServiceHostRequest>,
    ) -> super::Resp<api::MetricsServiceHostResponse> {
        self.trx(|c| host(req, c).scope_boxed()).await
    }
}

async fn node(
    req: tonic::Request<api::MetricsServiceNodeRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::MetricsServiceNodeResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::MetricsNode, conn).await?;
    let req = req.into_inner();
    let updates: Vec<models::UpdateNodeMetrics> = req
        .metrics
        .into_iter()
        .map(|(k, v)| v.as_metrics_update(&k))
        .collect::<crate::Result<_>>()?;
    let nodes = models::Node::find_by_ids(updates.iter().map(|u| u.id), conn).await?;
    let is_allowed = match claims.resource() {
        auth::Resource::User(user_id) => {
            let memberships = models::Org::memberships(user_id, conn).await?;
            let org_ids: HashSet<_> = memberships.into_iter().map(|ou| ou.org_id).collect();
            nodes.iter().all(|n| org_ids.contains(&n.org_id))
        }
        auth::Resource::Org(org_id) => nodes.iter().all(|n| n.org_id == org_id),
        auth::Resource::Host(host_id) => nodes.iter().all(|n| n.host_id == host_id),
        auth::Resource::Node(node_id) => nodes.iter().all(|n| n.id == node_id),
    };
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    models::UpdateNodeMetrics::update_metrics(updates, conn).await?;
    let resp = api::MetricsServiceNodeResponse {};
    Ok(tonic::Response::new(resp))
}

async fn host(
    req: tonic::Request<api::MetricsServiceHostRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::MetricsServiceHostResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::MetricsNode, conn).await?;
    let req = req.into_inner();
    let updates: Vec<models::UpdateHostMetrics> = req
        .metrics
        .into_iter()
        .map(|(k, v)| v.as_metrics_update(&k))
        .collect::<crate::Result<_>>()?;
    let hosts = models::Host::find_by_ids(updates.iter().map(|u| u.id), conn).await?;
    let is_allowed = match claims.resource() {
        auth::Resource::User(user_id) => {
            let memberships = models::Org::memberships(user_id, conn).await?;
            let org_ids: HashSet<_> = memberships.into_iter().map(|ou| ou.org_id).collect();
            hosts
                .iter()
                .all(|h: &models::Host| h.org_id.map(|id| org_ids.contains(&id)).unwrap_or(false))
        }
        auth::Resource::Org(org_id) => hosts.iter().all(|h| h.org_id == Some(org_id)),
        auth::Resource::Host(host_id) => hosts.iter().all(|h| h.id == host_id),
        auth::Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    models::UpdateHostMetrics::update_metrics(updates, conn).await?;
    let resp = api::MetricsServiceHostResponse {};
    Ok(tonic::Response::new(resp))
}

impl api::NodeMetrics {
    pub fn as_metrics_update(self, id: &str) -> crate::Result<models::UpdateNodeMetrics> {
        let id = id.parse()?;
        Ok(models::UpdateNodeMetrics {
            id,
            block_height: self.height.map(i64::try_from).transpose()?,
            block_age: self.block_age.map(i64::try_from).transpose()?,
            staking_status: Some(self.staking_status().into_model()),
            consensus: self.consensus,
            chain_status: Some(self.application_status().into_model()),
            sync_status: Some(self.sync_status().into_model()),
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
