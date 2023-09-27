pub mod key_file;
pub use key_file::NodeKeyFile;

pub mod log;
pub use log::{NewNodeLog, NodeLog, NodeLogEvent};

pub mod property;
pub use property::NodeProperty;

pub mod scheduler;
pub use scheduler::{NodeScheduler, ResourceAffinity, SimilarNodeAffinity};

pub mod status;
pub use status::{ContainerStatus, NodeChainStatus, NodeStakingStatus, NodeSyncStatus};

pub mod node_type;
pub use node_type::{NodeNetwork, NodeType, NodeVersion};

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use futures_util::future::OptionFuture;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tonic::Status;
use tracing::warn;

use crate::auth::resource::{HostId, NodeId, OrgId, UserId};
use crate::cookbook::identifier::Identifier;
use crate::database::{Conn, WriteConn};

use super::blockchain::{Blockchain, BlockchainId};
use super::host::{Host, HostRequirements, HostType};
use super::schema::nodes;
use super::{string_to_array, IpAddress, Paginate, Region, RegionId};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Blockchain error for node: {0}
    Blockchain(#[from] super::blockchain::Error),
    /// Cookbook error for node: {0}
    Cookbook(#[from] crate::cookbook::Error),
    /// Failed to create node: {0}
    Create(diesel::result::Error),
    /// Failed to delete node `{0}`: {1}
    Delete(NodeId, diesel::result::Error),
    /// Node DNS error: {0}
    Dns(#[from] crate::dns::Error),
    /// Failed to filter nodes: {0}
    Filter(diesel::result::Error),
    /// Failed to parse filtered IP addresses: {0}
    FilteredIps(serde_json::Error),
    /// Failed to find node by id `{0}`: {1}
    FindById(NodeId, diesel::result::Error),
    /// Failed to find nodes by id `{0:?}`: {1}
    FindByIds(HashSet<NodeId>, diesel::result::Error),
    /// Failed to find node ids `{0:?}`: {1}
    FindExistingIds(HashSet<NodeId>, diesel::result::Error),
    /// Failed to find outdated nodes: {0}
    FindOutdated(diesel::result::Error),
    /// Host error for node: {0}
    Host(#[from] super::host::Error),
    /// Only available host candidate failed.
    HostCandidateFailed,
    /// Failed to parse node limit as i64: {0}
    Limit(std::num::TryFromIntError),
    /// Failed to get next host ip for node: {0}
    NextHostIp(crate::models::ip_address::Error),
    /// Node log error: {0}
    NodeLog(#[from] log::Error),
    /// Node property error: {0}
    NodeProperty(property::Error),
    /// No available host candidates.
    NoHostCandidates,
    /// No available hosts.
    NoHosts,
    /// No host id or scheduler.
    NoHostOrScheduler,
    /// Failed to find a matching host.
    NoMatchingHost,
    /// Failed to parse node offset as i64: {0}
    Offset(std::num::TryFromIntError),
    /// Node region error: {0}
    Region(crate::models::region::Error),
    /// Failed to parse node total as i64: {0}
    Total(std::num::TryFromIntError),
    /// Failed to update node: {0}
    Update(diesel::result::Error),
    /// Failed to update node `{0}`: {1}
    UpdateById(NodeId, diesel::result::Error),
    /// Failed to update node {1}'s metrics: {0}
    UpdateMetrics(diesel::result::Error, NodeId),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            Delete(_, NotFound)
            | FindById(_, NotFound)
            | FindByIds(_, NotFound)
            | FindOutdated(NotFound) => Status::not_found("Not found."),
            NoMatchingHost => Status::resource_exhausted("No matching host."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Debug, Queryable, AsChangeset)]
pub struct Node {
    pub id: NodeId,
    pub org_id: OrgId,
    pub host_id: HostId,
    pub name: String,
    pub version: NodeVersion,
    pub ip_addr: String,
    pub address: Option<String>,
    pub wallet_address: Option<String>,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub blockchain_id: BlockchainId,
    pub sync_status: NodeSyncStatus,
    pub chain_status: NodeChainStatus,
    pub staking_status: Option<NodeStakingStatus>,
    pub container_status: ContainerStatus,
    pub ip_gateway: String,
    pub self_update: bool,
    pub block_age: Option<i64>,
    pub consensus: Option<bool>,
    pub vcpu_count: i64,
    pub mem_size_bytes: i64,
    pub disk_size_bytes: i64,
    pub host_name: String,
    pub network: String,
    pub created_by: Option<UserId>,
    pub dns_record_id: String,
    allow_ips: serde_json::Value,
    deny_ips: serde_json::Value,
    pub node_type: NodeType,
    pub scheduler_similarity: Option<SimilarNodeAffinity>,
    pub scheduler_resource: Option<ResourceAffinity>,
    pub scheduler_region: Option<RegionId>,
    pub data_directory_mountpoint: Option<String>,
    pub data_sync_progress_total: Option<i32>,
    pub data_sync_progress_current: Option<i32>,
    pub data_sync_progress_message: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NodeFilter {
    pub org_id: OrgId,
    pub offset: u64,
    pub limit: u64,
    pub status: Vec<NodeChainStatus>,
    pub node_types: Vec<NodeType>,
    pub blockchains: Vec<BlockchainId>,
    pub host_id: Option<HostId>,
}

impl Node {
    pub async fn find_by_id(id: NodeId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        nodes::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn find_by_ids(
        ids: HashSet<NodeId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        nodes::table
            .filter(nodes::id.eq_any(ids.iter()))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByIds(ids, err))
    }

    /// Filters out any node ids that do no exist.
    pub async fn existing_ids(
        ids: HashSet<NodeId>,
        conn: &mut Conn<'_>,
    ) -> Result<HashSet<NodeId>, Error> {
        let ids = nodes::table
            .filter(nodes::id.eq_any(ids.iter()))
            .select(nodes::id)
            .get_results(conn)
            .await
            .map_err(|err| Error::FindExistingIds(ids, err))?;
        Ok(ids.into_iter().collect())
    }

    pub async fn properties(&self, conn: &mut Conn<'_>) -> Result<Vec<NodeProperty>, Error> {
        NodeProperty::by_node_id(self.id, conn)
            .await
            .map_err(Error::NodeProperty)
    }

    pub async fn filter(
        filter: NodeFilter,
        conn: &mut Conn<'_>,
    ) -> Result<(u64, Vec<Self>), Error> {
        let NodeFilter {
            org_id,
            offset,
            limit,
            status,
            node_types,
            blockchains,
            host_id,
        } = filter;

        let mut query = nodes::table.filter(nodes::org_id.eq(org_id)).into_boxed();

        // Apply filters if present
        if !blockchains.is_empty() {
            query = query.filter(nodes::blockchain_id.eq_any(blockchains));
        }

        if !status.is_empty() {
            query = query.filter(nodes::chain_status.eq_any(status));
        }

        if !node_types.is_empty() {
            query = query.filter(nodes::node_type.eq_any(node_types));
        }

        if let Some(host_id) = host_id {
            query = query.filter(nodes::host_id.eq(host_id));
        }

        let limit = i64::try_from(limit).map_err(Error::Limit)?;
        let offset = i64::try_from(offset).map_err(Error::Offset)?;

        let (total, nodes) = query
            .order_by(nodes::created_at.desc())
            .paginate(limit, offset)
            .get_results_counted(conn)
            .await
            .map_err(Error::Filter)?;

        let total = u64::try_from(total).map_err(Error::Total)?;

        Ok((total, nodes))
    }

    pub async fn update(self, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let mut updated = self.clone();
        updated.updated_at = Utc::now();

        diesel::update(nodes::table.find(updated.id))
            .set(updated)
            .get_result(conn)
            .await
            .map_err(|err| Error::UpdateById(self.id, err))
    }

    pub async fn delete(id: NodeId, write: &mut WriteConn<'_, '_>) -> Result<(), Error> {
        let node = Self::find_by_id(id, write).await?;

        diesel::delete(nodes::table.find(id))
            .execute(write)
            .await
            .map_err(|err| Error::Delete(id, err))?;

        if let Err(err) = write.ctx.dns.remove_node_dns(&node.dns_record_id).await {
            warn!("Failed to remove node dns: {err}");
        }

        Ok(())
    }

    /// Finds the next possible host for this node to be tried on.
    pub async fn find_host(&self, write: &mut WriteConn<'_, '_>) -> Result<Host, Error> {
        let chain = Blockchain::find_by_id(self.blockchain_id, write).await?;

        let id = Identifier::new(chain.name, self.node_type, self.version.clone());
        let meta = write.ctx.cookbook.rhai_metadata(&id).await?;

        let candidates = match self.scheduler(write).await? {
            Some(scheduler) => {
                let reqs = HostRequirements {
                    requirements: meta.requirements,
                    blockchain_id: self.blockchain_id,
                    node_type: self.node_type,
                    host_type: Some(HostType::Cloud),
                    scheduler,
                    org_id: None,
                };
                Host::host_candidates(reqs, Some(2), write).await?
            }
            None => vec![Host::find_by_id(self.host_id, write).await?],
        };

        // We now have a list of host candidates for our nodes. Now the only thing left to do is to
        // make a decision about where to place the node.
        let deployments = NodeLog::by_node(self, write).await?;
        let hosts_tried = NodeLog::hosts_tried(&deployments, write).await?;
        let best = match (hosts_tried.as_slice(), candidates.len()) {
            // If there are 0 hosts to try, we return an error.
            (_, 0) => return Err(Error::NoHostCandidates),
            // If we are on the first host to try we just take the first candidate.
            ([], _) => candidates[0].clone(),
            // If we are on the first host to try and we tried once, we try that host again.
            ([(host, 1)], 1) => host.clone(),
            // Now we need at least two candidates, so lets check for that.
            (_, 1) => return Err(Error::HostCandidateFailed),
            // If there is 1 host that we tried so far, we can try a new one
            ([_], _) => candidates[1].clone(),
            // If we are on the second host to try and we tried once, we try that host again.
            ([_, (host, 1)], _) => host.clone(),
            // Otherwise we exhausted our our options and return an error.
            (_, _) => return Err(Error::NoHosts),
        };

        Ok(best)
    }

    pub async fn scheduler(&self, conn: &mut Conn<'_>) -> Result<Option<NodeScheduler>, Error> {
        let Some(resource) = self.scheduler_resource else {
            return Ok(None);
        };
        Ok(Some(NodeScheduler {
            region: self.region(conn).await?,
            similarity: self.scheduler_similarity,
            resource,
        }))
    }

    pub async fn region(&self, conn: &mut Conn<'_>) -> Result<Option<Region>, Error> {
        let region = self.scheduler_region.map(|r| Region::by_id(r, conn));
        OptionFuture::from(region)
            .await
            .transpose()
            .map_err(Error::Region)
    }

    pub fn allow_ips(&self) -> Result<Vec<FilteredIpAddr>, Error> {
        Self::filtered_ip_addrs(self.allow_ips.clone())
    }

    pub fn deny_ips(&self) -> Result<Vec<FilteredIpAddr>, Error> {
        Self::filtered_ip_addrs(self.deny_ips.clone())
    }

    /// Returns all nodes that are ready to be upgraded for this variant of a blockchain.
    pub async fn outdated(
        blockchain_id: BlockchainId,
        version: &str,
        node_type: NodeType,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        nodes::table
            .filter(nodes::self_update)
            .filter(nodes::node_type.eq(node_type))
            .filter(string_to_array(nodes::version, ".").lt(string_to_array(&version, ".")))
            .filter(nodes::blockchain_id.eq(blockchain_id))
            .get_results(conn)
            .await
            .map_err(Error::FindOutdated)
    }

    fn filtered_ip_addrs(value: serde_json::Value) -> Result<Vec<FilteredIpAddr>, Error> {
        serde_json::from_value(value).map_err(Error::FilteredIps)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilteredIpAddr {
    pub ip: String,
    pub description: Option<String>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = nodes)]
pub struct NewNode {
    pub id: NodeId,
    pub org_id: OrgId,
    pub name: String,
    pub version: NodeVersion,
    pub blockchain_id: BlockchainId,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub chain_status: NodeChainStatus,
    pub sync_status: NodeSyncStatus,
    pub staking_status: NodeStakingStatus,
    pub container_status: ContainerStatus,
    pub self_update: bool,
    pub vcpu_count: i64,
    pub mem_size_bytes: i64,
    pub disk_size_bytes: i64,
    pub network: NodeNetwork,
    pub allow_ips: serde_json::Value,
    pub deny_ips: serde_json::Value,
    pub node_type: NodeType,
    pub created_by: UserId,
    /// Controls whether to run the node on hosts that contain nodes similar to this one.
    pub scheduler_similarity: Option<SimilarNodeAffinity>,
    /// Controls whether to run the node on hosts that are full or empty.
    pub scheduler_resource: Option<ResourceAffinity>,
    /// The region where this node should be deployed.
    pub scheduler_region: Option<RegionId>,
}

impl NewNode {
    pub async fn create(
        self,
        host: Option<Host>,
        mut write: &mut WriteConn<'_, '_>,
    ) -> Result<Node, Error> {
        let host = match host {
            Some(host) => host,
            None => {
                let scheduler = self
                    .scheduler(write)
                    .await?
                    .ok_or(Error::NoHostOrScheduler)?;
                self.find_host(scheduler, write).await?
            }
        };
        let ip_addr = IpAddress::next_for_host(host.id, write)
            .await
            .map_err(Error::NextHostIp)?
            .ip
            .ip()
            .to_string();

        let ip_gateway = host.ip_gateway.ip().to_string();

        let blockchain = Blockchain::find_by_id(self.blockchain_id, write).await?;
        let dns_record_id = write
            .ctx
            .dns
            .get_node_dns(&self.name, ip_addr.clone())
            .await?;

        let id = Identifier::new(blockchain.name, self.node_type, self.version.clone());
        let meta = write.ctx.cookbook.rhai_metadata(&id).await?;
        let data_directory_mountpoint = meta
            .babel_config
            .and_then(|cfg| cfg.data_directory_mount_point);

        diesel::insert_into(nodes::table)
            .values((
                self,
                nodes::host_id.eq(host.id),
                nodes::ip_gateway.eq(ip_gateway),
                nodes::ip_addr.eq(ip_addr),
                nodes::host_name.eq(&host.name),
                nodes::dns_record_id.eq(dns_record_id),
                nodes::data_directory_mountpoint.eq(data_directory_mountpoint),
            ))
            .get_result::<Node>(&mut write)
            .await
            .map_err(Error::Create)
    }

    /// Finds the most suitable host to initially place the node on.
    ///
    /// Since this is a freshly created node, we do not need to worry about
    /// logic regarding where to retry placing the node. We simply ask for an
    /// ordered list of the most suitable hosts, and pick the first one.
    pub async fn find_host(
        &self,
        scheduler: NodeScheduler,
        write: &mut WriteConn<'_, '_>,
    ) -> Result<Host, Error> {
        let chain = Blockchain::find_by_id(self.blockchain_id, write).await?;

        let id = Identifier::new(chain.name, self.node_type, self.version.clone());
        let metadata = write.ctx.cookbook.rhai_metadata(&id).await?;

        let requirements = HostRequirements {
            requirements: metadata.requirements,
            blockchain_id: self.blockchain_id,
            node_type: self.node_type,
            host_type: Some(HostType::Cloud),
            scheduler,
            org_id: None,
        };

        let candidates = Host::host_candidates(requirements, Some(1), write).await?;
        candidates.into_iter().next().ok_or(Error::NoMatchingHost)
    }

    async fn scheduler(&self, conn: &mut Conn<'_>) -> Result<Option<NodeScheduler>, Error> {
        let Some(resource) = self.scheduler_resource else {
            return Ok(None);
        };
        let region = self.scheduler_region.map(|id| Region::by_id(id, conn));
        let region = OptionFuture::from(region)
            .await
            .transpose()
            .map_err(Error::Region)?;

        Ok(Some(NodeScheduler {
            region,
            similarity: self.scheduler_similarity,
            resource,
        }))
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = nodes)]
pub struct UpdateNode<'a> {
    pub id: NodeId,
    pub name: Option<&'a str>,
    pub version: Option<&'a str>,
    pub ip_addr: Option<&'a str>,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub chain_status: Option<NodeChainStatus>,
    pub sync_status: Option<NodeSyncStatus>,
    pub staking_status: Option<NodeStakingStatus>,
    pub container_status: Option<ContainerStatus>,
    pub self_update: Option<bool>,
    pub address: Option<&'a str>,
    pub allow_ips: Option<serde_json::Value>,
    pub deny_ips: Option<serde_json::Value>,
}

impl UpdateNode<'_> {
    pub async fn update(&self, conn: &mut Conn<'_>) -> Result<Node, Error> {
        diesel::update(nodes::table.find(self.id))
            .set((self, nodes::updated_at.eq(Utc::now())))
            .get_result(conn)
            .await
            .map_err(Error::Update)
    }
}

/// Update node columns related to metrics.
#[derive(Debug, Insertable, AsChangeset)]
#[diesel(table_name = nodes)]
pub struct UpdateNodeMetrics {
    pub id: NodeId,
    pub block_height: Option<i64>,
    pub block_age: Option<i64>,
    pub staking_status: Option<NodeStakingStatus>,
    pub consensus: Option<bool>,
    pub chain_status: Option<NodeChainStatus>,
    pub sync_status: Option<NodeSyncStatus>,
    pub data_sync_progress_total: Option<i32>,
    pub data_sync_progress_current: Option<i32>,
    pub data_sync_progress_message: Option<String>,
}

impl UpdateNodeMetrics {
    pub async fn update_metrics(
        mut updates: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Node>, Error> {
        // We do this for determinism in our tests.
        updates.sort_by_key(|u| u.id);

        let mut nodes = Vec::with_capacity(updates.len());
        for update in updates {
            let updated = diesel::update(nodes::table.find(update.id))
                .set(&update)
                .get_result(conn)
                .await
                .map_err(|err| Error::UpdateMetrics(err, update.id))?;
            nodes.push(updated);
        }
        Ok(nodes)
    }
}

#[cfg(test)]
mod tests {
    use petname::petname;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use crate::config::Context;

    use super::*;

    #[tokio::test]
    async fn can_filter_nodes() {
        let (ctx, db) = Context::with_mocked().await.unwrap();
        let name = format!("test_{}", petname(3, "_"));

        let blockchain_id = db.seed.blockchain.id;
        let user_id = db.seed.user.id;
        let org_id = db.seed.org.id;

        let req = NewNode {
            id: Uuid::new_v4().into(),
            org_id,
            blockchain_id,
            chain_status: NodeChainStatus::Unknown,
            sync_status: NodeSyncStatus::Syncing,
            container_status: ContainerStatus::Installing,
            block_height: None,
            node_data: None,
            name,
            version: "3.3.0".to_string().into(),
            staking_status: NodeStakingStatus::Staked,
            self_update: false,
            vcpu_count: 0,
            mem_size_bytes: 0,
            disk_size_bytes: 0,
            network: "some network".to_string().into(),
            node_type: NodeType::Validator,
            created_by: user_id,
            scheduler_similarity: None,
            scheduler_resource: Some(ResourceAffinity::MostResources),
            scheduler_region: None,
            allow_ips: serde_json::json!([]),
            deny_ips: serde_json::json!([]),
        };

        let (meta_tx, _meta_rx) = mpsc::unbounded_channel();
        let (mqtt_tx, _mqtt_rx) = mpsc::unbounded_channel();
        let mut write = WriteConn {
            conn: &mut db.conn().await,
            ctx: &ctx,
            meta_tx,
            mqtt_tx,
        };

        let host = db.seed.host.clone();
        let host_id = db.seed.host.id;
        req.create(Some(host), &mut write).await.unwrap();

        let filter = NodeFilter {
            status: vec![NodeChainStatus::Unknown],
            node_types: vec![],
            blockchains: vec![blockchain_id],
            limit: 10,
            offset: 0,
            org_id,
            host_id: Some(host_id),
        };

        let (_, nodes) = Node::filter(filter, &mut write).await.unwrap();

        assert_eq!(nodes.len(), 1);
    }
}
