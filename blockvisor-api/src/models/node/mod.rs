pub mod job;
pub use job::{NodeJob, NodeJobProgress, NodeJobStatus};

pub mod log;
pub use log::{NewNodeLog, NodeLog, NodeLogEvent};

pub mod property;
pub use property::NodeProperty;

pub mod scheduler;
pub use scheduler::{NodeScheduler, ResourceAffinity, SimilarNodeAffinity};

pub mod status;
pub use status::{ContainerStatus, NodeStakingStatus, NodeStatus, NodeSyncStatus};

pub mod node_type;
pub use node_type::{NodeNetwork, NodeType, NodeVersion};

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel::{dsl, prelude::*};
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use futures_util::future::OptionFuture;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tonic::Status;
use tracing::warn;

use crate::auth::resource::{HostId, NodeId, OrgId, ResourceId, ResourceType};
use crate::cookbook::image::Image;
use crate::database::{Conn, WriteConn};
use crate::util::SearchOperator;

use super::blockchain::{Blockchain, BlockchainId};
use super::host::{Host, HostRequirements, HostType};
use super::schema::nodes;
use super::{IpAddress, Paginate, Region, RegionId};

type NotDeleted = dsl::Filter<nodes::table, dsl::IsNull<nodes::deleted_at>>;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to assign ip address to node: {0},
    AssignIpAddr(#[from] super::ip_address::Error),
    /// Blockchain error for node: {0}
    Blockchain(#[from] super::blockchain::Error),
    /// Command error: {0}
    Command(Box<super::command::Error>),
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
    /// Failed to find nodes by host id {0}: {1}
    FindByHostId(HostId, diesel::result::Error),
    /// Failed to find node by id `{0}`: {1}
    FindById(NodeId, diesel::result::Error),
    /// Failed to find nodes by id `{0:?}`: {1}
    FindByIds(HashSet<NodeId>, diesel::result::Error),
    /// Failed to find nodes by org id {0}: {1}
    FindByOrgId(OrgId, diesel::result::Error),
    /// Failed to find node ids `{0:?}`: {1}
    FindExistingIds(HashSet<NodeId>, diesel::result::Error),
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
    /// Failed to parse IpAddr: {0}
    ParseIpAddr(std::net::AddrParseError),
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
    /// Failed to find upgradeable nodes for blockchain id `{0}` and node type `{1}`: {2}
    UpgradeableByType(BlockchainId, NodeType, diesel::result::Error),
    /// Failed to parse the jobs column of the node table: {0}
    UnparsableJobs(serde_json::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            Delete(_, NotFound)
            | FindById(_, NotFound)
            | FindByIds(_, NotFound)
            | UpgradeableByType(_, _, NotFound) => Status::not_found("Not found."),
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
    pub node_status: NodeStatus,
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
    pub created_by: Option<ResourceId>,
    pub dns_record_id: String,
    allow_ips: serde_json::Value,
    deny_ips: serde_json::Value,
    pub node_type: NodeType,
    pub scheduler_similarity: Option<SimilarNodeAffinity>,
    pub scheduler_resource: Option<ResourceAffinity>,
    pub scheduler_region: Option<RegionId>,
    pub data_directory_mountpoint: Option<String>,
    pub jobs: serde_json::Value,
    pub created_by_resource: Option<ResourceType>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub struct NodeFilter {
    pub org_id: Option<OrgId>,
    pub offset: u64,
    pub limit: u64,
    pub status: Vec<NodeStatus>,
    pub node_types: Vec<NodeType>,
    pub blockchains: Vec<BlockchainId>,
    pub host_id: Option<HostId>,
    pub search: Option<NodeSearch>,
}

#[derive(Debug)]
pub struct NodeSearch {
    pub operator: SearchOperator,
    pub id: Option<String>,
    pub name: Option<String>,
    pub ip: Option<String>,
}

// These statuses imply that a node is deleted.
const DELETED_STATUSES: [NodeStatus; 3] = [
    NodeStatus::DeletePending,
    NodeStatus::Deleting,
    NodeStatus::Deleted,
];

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

    pub async fn find_by_org(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        Self::not_deleted()
            .filter(nodes::org_id.eq(org_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByOrgId(org_id, err))
    }

    pub async fn find_by_host(host_id: HostId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        Self::not_deleted()
            .filter(nodes::host_id.eq(host_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByHostId(host_id, err))
    }

    pub async fn upgradeable_by_type(
        blockchain_id: BlockchainId,
        node_type: NodeType,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        Self::not_deleted()
            .filter(nodes::blockchain_id.eq(blockchain_id))
            .filter(nodes::node_type.eq(node_type))
            .filter(nodes::self_update)
            .get_results(conn)
            .await
            .map_err(|err| Error::UpgradeableByType(blockchain_id, node_type, err))
    }

    /// Filters out any node ids that do no exist.
    pub async fn existing_ids(
        ids: HashSet<NodeId>,
        conn: &mut Conn<'_>,
    ) -> Result<HashSet<NodeId>, Error> {
        let ids = Node::not_deleted()
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
            search,
        } = filter;

        let mut query = nodes::table.into_boxed();

        // search fields
        if let Some(search) = search {
            let NodeSearch {
                operator,
                id,
                name,
                ip,
            } = search;
            match operator {
                SearchOperator::Or => {
                    if let Some(id) = id {
                        query = query.filter(super::text(nodes::id).like(id));
                    }
                    if let Some(name) = name {
                        query = query.or_filter(super::lower(nodes::name).like(name));
                    }
                    if let Some(ip) = ip {
                        query = query.or_filter(nodes::ip_addr.like(ip));
                    }
                }
                SearchOperator::And => {
                    if let Some(id) = id {
                        query = query.filter(super::text(nodes::id).like(id));
                    }
                    if let Some(name) = name {
                        query = query.filter(super::lower(nodes::name).like(name));
                    }
                    if let Some(ip) = ip {
                        query = query.filter(nodes::ip_addr.like(ip));
                    }
                }
            }
        }

        if let Some(org_id) = org_id {
            query = query.filter(nodes::org_id.eq(org_id));
        }

        if !blockchains.is_empty() {
            query = query.filter(nodes::blockchain_id.eq_any(blockchains));
        }

        // If the user requested a deleted status, we include deleted records with the response.
        // Conversely, if no such request is made, we exlude all deleted rows.
        if !status.iter().any(|s| DELETED_STATUSES.contains(s)) {
            query = query.filter(nodes::deleted_at.is_null());
        }
        if !status.is_empty() {
            query = query.filter(nodes::node_status.eq_any(status));
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

        diesel::update(nodes::table.find(id))
            .set(nodes::deleted_at.eq(Utc::now()))
            .execute(write)
            .await
            .map_err(|err| Error::Delete(id, err))?;

        let ip_addr = node.ip_addr.parse().map_err(Error::ParseIpAddr)?;
        let ip = IpAddress::find_by_node(ip_addr, write).await?;

        IpAddress::unassign(ip.id, node.host_id, write).await?;

        // Delete all pending commands for this node: there are not useable anymore
        super::Command::delete_pending(node.id, write)
            .await
            .map_err(|err| Error::Command(Box::new(err)))?;

        if let Err(err) = write.ctx.dns.remove_node_dns(&node.dns_record_id).await {
            warn!("Failed to remove node dns: {err}");
        }

        Ok(())
    }

    /// Finds the next possible host for this node to be tried on.
    pub async fn find_host(&self, write: &mut WriteConn<'_, '_>) -> Result<Host, Error> {
        let chain = Blockchain::find_by_id(self.blockchain_id, write).await?;

        let image = Image::new(chain.name, self.node_type, self.version.clone());
        let meta = write.ctx.cookbook.rhai_metadata(&image).await?;

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

    fn filtered_ip_addrs(value: serde_json::Value) -> Result<Vec<FilteredIpAddr>, Error> {
        serde_json::from_value(value).map_err(Error::FilteredIps)
    }

    pub fn jobs(&self) -> Result<Vec<NodeJob>, Error> {
        serde_json::from_value(self.jobs.clone()).map_err(Error::UnparsableJobs)
    }

    pub fn not_deleted() -> NotDeleted {
        nodes::table.filter(nodes::deleted_at.is_null())
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
    pub node_status: NodeStatus,
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
    pub created_by: ResourceId,
    pub created_by_resource: ResourceType,
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
        let host = if let Some(host) = host {
            host
        } else {
            let scheduler = self
                .scheduler(write)
                .await?
                .ok_or(Error::NoHostOrScheduler)?;
            self.find_host(scheduler, write).await?
        };

        let ip_addr = IpAddress::next_for_host(host.id, write)
            .await
            .map_err(Error::NextHostIp)?;
        IpAddress::assign(ip_addr.id, host.id, write)
            .await
            .map_err(Error::AssignIpAddr)?;

        let ip_gateway = host.ip_gateway.ip().to_string();

        let blockchain = Blockchain::find_by_id(self.blockchain_id, write).await?;
        let dns_record_id = write.ctx.dns.get_node_dns(&self.name, ip_addr.ip()).await?;

        let image = Image::new(blockchain.name, self.node_type, self.version.clone());
        let meta = write.ctx.cookbook.rhai_metadata(&image).await?;
        let data_directory_mountpoint = meta
            .babel_config
            .and_then(|cfg| cfg.data_directory_mount_point);

        diesel::insert_into(nodes::table)
            .values((
                self,
                nodes::host_id.eq(host.id),
                nodes::ip_gateway.eq(ip_gateway),
                nodes::ip_addr.eq(ip_addr.ip()),
                nodes::host_name.eq(&host.name),
                nodes::dns_record_id.eq(dns_record_id),
                nodes::data_directory_mountpoint.eq(data_directory_mountpoint),
            ))
            .get_result(&mut write)
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

        let image = Image::new(chain.name, self.node_type, self.version.clone());
        let metadata = write.ctx.cookbook.rhai_metadata(&image).await?;

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
    pub node_status: Option<NodeStatus>,
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
    pub node_status: Option<NodeStatus>,
    pub sync_status: Option<NodeSyncStatus>,
    pub jobs: Option<serde_json::Value>,
}

impl UpdateNodeMetrics {
    pub async fn update_metrics(
        mut updates: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Node>, Error> {
        // We do this for determinism in our tests.
        updates.sort_by_key(|u| u.id);

        let mut results = Vec::with_capacity(updates.len());
        for update in updates {
            let updated = diesel::update(Node::not_deleted().find(update.id))
                .set(&update)
                .get_result(conn)
                .await
                .map_err(|err| Error::UpdateMetrics(err, update.id))?;
            results.push(updated);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use crate::config::Context;

    use super::*;

    #[tokio::test]
    async fn can_filter_nodes() {
        let (ctx, db) = Context::with_mocked().await.unwrap();
        let name = format!("test_{}", petname::petname(3, "_").unwrap());

        let blockchain_id = db.seed.blockchain.id;
        let user_id = db.seed.user.id;
        let org_id = db.seed.org.id;

        let req = NewNode {
            id: Uuid::new_v4().into(),
            org_id,
            blockchain_id,
            node_status: NodeStatus::Ingesting,
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
            created_by: user_id.into(),
            created_by_resource: ResourceType::User,
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
            status: vec![NodeStatus::Ingesting],
            node_types: vec![],
            blockchains: vec![blockchain_id],
            limit: 10,
            offset: 0,
            org_id: Some(org_id),
            host_id: Some(host_id),
            search: None,
        };

        let (_, nodes) = Node::filter(filter, &mut write).await.unwrap();

        assert_eq!(nodes.len(), 1);
    }
}
