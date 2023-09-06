//! The metrics service is the service that relates to the metrics for nodes and hosts that we
//! gather. At some point we may switch to a provisioned metrics service, so for now this service
//! does not store a history of metrics. Rather, it overwrites the metrics that are know for each
//! time new ones are provided. This makes sure that the database doesn't grow overly large.

use std::collections::HashSet;

use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::endpoint::Endpoint;
use crate::auth::resource::Resource;
use crate::database::{Transaction, WriteConn};
use crate::models::host::{Host, UpdateHostMetrics};
use crate::models::node::{Node, UpdateNodeMetrics};
use crate::models::org::Org;

use super::api::{self, metrics_service_server};

#[tonic::async_trait]
impl metrics_service_server::MetricsService for super::Grpc {
    /// Update the metrics for the nodes provided in this request. Since this endpoint is called
    /// often (e.g. if we have 10,000 nodes, 170 calls per second) we take care to perform a single
    /// query for this whole list of metrics that comes in.
    async fn node(
        &self,
        req: tonic::Request<api::MetricsServiceNodeRequest>,
    ) -> super::Resp<api::MetricsServiceNodeResponse> {
        self.write(|write| node(req, write).scope_boxed()).await
    }

    async fn host(
        &self,
        req: tonic::Request<api::MetricsServiceHostRequest>,
    ) -> super::Resp<api::MetricsServiceHostResponse> {
        self.write(|write| host(req, write).scope_boxed()).await
    }
}

async fn node(
    req: tonic::Request<api::MetricsServiceNodeRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::MetricsServiceNodeResponse> {
    let WriteConn { conn, ctx, mqtt_tx } = write;
    let claims = ctx.claims(&req, Endpoint::MetricsNode, conn).await?;
    let req = req.into_inner();
    let updates: Vec<UpdateNodeMetrics> = req
        .metrics
        .into_iter()
        .map(|(k, v)| v.as_metrics_update(&k))
        .collect::<crate::Result<_>>()?;
    let node_ids = updates.iter().map(|u| u.id).collect();
    let nodes = Node::find_by_ids(node_ids, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => {
            let memberships = Org::memberships(user_id, conn).await?;
            let org_ids: HashSet<_> = memberships.into_iter().map(|ou| ou.org_id).collect();
            nodes.iter().all(|n| org_ids.contains(&n.org_id))
        }
        Resource::Org(org_id) => nodes.iter().all(|n| n.org_id == org_id),
        Resource::Host(host_id) => nodes.iter().all(|n| n.host_id == host_id),
        Resource::Node(node_id) => nodes.iter().all(|n| n.id == node_id),
    };
    if !is_allowed {
        super::forbidden!("Access denied for metrics node");
    }

    let nodes = UpdateNodeMetrics::update_metrics(updates, conn).await?;
    api::NodeMessage::updated_many(nodes, conn)
        .await?
        .into_iter()
        .for_each(|msg| mqtt_tx.send(msg.into()).expect("mqtt_rx"));

    let resp = api::MetricsServiceNodeResponse {};

    Ok(tonic::Response::new(resp))
}

async fn host(
    req: tonic::Request<api::MetricsServiceHostRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::MetricsServiceHostResponse> {
    let WriteConn { conn, ctx, mqtt_tx } = write;
    let claims = ctx.claims(&req, Endpoint::MetricsNode, conn).await?;
    let req = req.into_inner();
    let updates: Vec<UpdateHostMetrics> = req
        .metrics
        .into_iter()
        .map(|(k, v)| v.as_metrics_update(&k))
        .collect::<crate::Result<_>>()?;
    let host_ids = updates.iter().map(|u| u.id).collect();
    let hosts = Host::find_by_ids(host_ids, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => {
            let memberships = Org::memberships(user_id, conn).await?;
            let org_ids: HashSet<_> = memberships.into_iter().map(|ou| ou.org_id).collect();
            hosts.iter().all(|h| org_ids.contains(&h.org_id))
        }
        Resource::Org(org_id) => hosts.iter().all(|h| h.org_id == org_id),
        Resource::Host(host_id) => hosts.iter().all(|h| h.id == host_id),
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for metrics host");
    }

    let hosts = UpdateHostMetrics::update_metrics(updates, conn).await?;
    api::HostMessage::updated_many(hosts, &claims, conn)
        .await?
        .into_iter()
        .for_each(|msg| mqtt_tx.send(msg.into()).expect("mqtt_rx"));

    let resp = api::MetricsServiceHostResponse {};

    Ok(tonic::Response::new(resp))
}

impl api::NodeMetrics {
    pub fn as_metrics_update(self, id: &str) -> crate::Result<UpdateNodeMetrics> {
        let id = id.parse()?;
        Ok(UpdateNodeMetrics {
            id,
            block_height: self.height.map(i64::try_from).transpose()?,
            block_age: self.block_age.map(i64::try_from).transpose()?,
            staking_status: Some(self.staking_status().into_model()),
            consensus: self.consensus,
            chain_status: Some(self.application_status().into_model()),
            sync_status: Some(self.sync_status().into_model()),
            data_sync_progress_total: self
                .data_sync_progress_total
                .map(i32::try_from)
                .transpose()?,
            data_sync_progress_current: self
                .data_sync_progress_current
                .map(i32::try_from)
                .transpose()?,
            data_sync_progress_message: self.data_sync_progress_message,
        })
    }
}

impl api::HostMetrics {
    pub fn as_metrics_update(self, id: &str) -> crate::Result<UpdateHostMetrics> {
        let id = id.parse()?;
        Ok(UpdateHostMetrics {
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
