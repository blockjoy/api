pub mod job;
pub use job::{NodeJob, NodeJobProgress, NodeJobStatus};

pub mod log;
pub use log::{NewNodeLog, NodeLog, NodeLogEvent};

pub mod node_type;
pub use node_type::{NodeNetwork, NodeType, NodeVersion};

pub mod property;
pub use property::NodeProperty;

pub mod report;
pub use report::{NewNodeReport, NodeReport, NodeReportId};

pub mod scheduler;
pub use scheduler::{NodeScheduler, ResourceAffinity, SimilarNodeAffinity};

pub mod status;
pub use status::{ContainerStatus, NodeStatus, StakingStatus, SyncStatus};

use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Utc};
use diesel::dsl::{InnerJoinQuerySource, LeftJoinQuerySource};
use diesel::expression::expression_types::NotSelectable;
use diesel::pg::Pg;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel::sql_types::Bool;
use diesel::{dsl, prelude::*};
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use futures_util::future::OptionFuture;
use petname::{Generator, Petnames};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tonic::Status;
use tracing::warn;

use crate::auth::resource::{
    HostId, NodeId, OrgId, Resource, ResourceEntry, ResourceId, ResourceType, UserId,
};
use crate::auth::AuthZ;
use crate::database::{Conn, WriteConn};
use crate::storage::image::ImageId;
use crate::util::{SearchOperator, SortOrder};

use super::blockchain::{Blockchain, BlockchainId};
use super::host::{Host, HostRequirements, HostType};
use super::schema::{hosts, nodes, regions};
use super::{Command, IpAddress, Org, Paginate, Region, RegionId};

type NotDeleted = dsl::Filter<nodes::table, dsl::IsNull<nodes::deleted_at>>;

const DELETED_STATUSES: [NodeStatus; 3] = [
    NodeStatus::DeletePending,
    NodeStatus::Deleting,
    NodeStatus::Deleted,
];

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to assign ip address to node: {0},
    AssignIpAddr(#[from] super::ip_address::Error),
    /// Blockchain error for node: {0}
    Blockchain(#[from] super::blockchain::Error),
    /// Node Command error: {0}
    Command(Box<super::command::Error>),
    /// Node Cloudflare error: {0}
    Cloudflare(#[from] crate::cloudflare::Error),
    /// Failed to create node: {0}
    Create(diesel::result::Error),
    /// Failed to delete node `{0}`: {1}
    Delete(NodeId, diesel::result::Error),
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
    /// Node org error: {0}
    Org(#[from] crate::models::org::Error),
    /// Node pagination: {0}
    Paginate(#[from] crate::models::paginate::Error),
    /// Failed to parse IpAddr: {0}
    ParseIpAddr(std::net::AddrParseError),
    /// Node region error: {0}
    Region(crate::models::region::Error),
    /// Failed to regenerate node name. This should not happen.
    RegenerateName,
    /// Node report error: {0}
    Report(report::Error),
    /// Storage error for node: {0}
    Storage(#[from] crate::storage::Error),
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
            Org(err) => err.into(),
            Paginate(err) => err.into(),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Debug, Queryable, AsChangeset, Selectable)]
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
    pub ip_gateway: String,
    pub self_update: bool,
    pub block_age: Option<i64>,
    pub consensus: Option<bool>,
    pub vcpu_count: i64,
    pub mem_size_bytes: i64,
    pub disk_size_bytes: i64,
    pub network: String,
    pub created_by: Option<ResourceId>,
    pub dns_record_id: String,
    allow_ips: serde_json::Value,
    deny_ips: serde_json::Value,
    pub scheduler_similarity: Option<SimilarNodeAffinity>,
    pub scheduler_resource: Option<ResourceAffinity>,
    pub scheduler_region: Option<RegionId>,
    pub data_directory_mountpoint: Option<String>,
    pub jobs: serde_json::Value,
    pub created_by_resource: Option<ResourceType>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub node_type: NodeType,
    pub container_status: ContainerStatus,
    pub sync_status: SyncStatus,
    pub staking_status: Option<StakingStatus>,
    pub note: Option<String>,
    pub node_status: NodeStatus,
    pub url: String,
}

impl Node {
    pub async fn by_id(id: NodeId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        nodes::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn by_ids(ids: HashSet<NodeId>, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        nodes::table
            .filter(nodes::id.eq_any(ids.iter()))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByIds(ids, err))
    }

    pub async fn by_org_id(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        Self::not_deleted()
            .filter(nodes::org_id.eq(org_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByOrgId(org_id, err))
    }

    pub async fn by_host_id(host_id: HostId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
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

    pub async fn update(self, update: UpdateNode<'_>, conn: &mut Conn<'_>) -> Result<Node, Error> {
        if let Some(new_org_id) = update.org_id {
            if new_org_id != self.org_id {
                let new_node_log = NewNodeLog {
                    host_id: self.host_id,
                    node_id: self.id,
                    event: NodeLogEvent::TransferredToOrg,
                    blockchain_id: self.blockchain_id,
                    node_type: self.node_type,
                    version: self.version.clone(),
                    created_at: Utc::now(),
                    org_id: new_org_id,
                };
                new_node_log.create(conn).await?;
            }
        }

        diesel::update(nodes::table.find(self.id))
            .set((update, nodes::updated_at.eq(Utc::now())))
            .get_result(conn)
            .await
            .map_err(Error::Update)
    }

    pub async fn delete(id: NodeId, write: &mut WriteConn<'_, '_>) -> Result<(), Error> {
        let node: Node = diesel::update(nodes::table.find(id))
            .set(nodes::deleted_at.eq(Utc::now()))
            .get_result(write)
            .await
            .map_err(|err| Error::Delete(id, err))?;

        node.ip(write).await?.unassign(write).await?;

        Org::decrement_node(node.org_id, write).await?;
        Host::decrement_node(node.host_id, write).await?;

        Command::delete_node_pending(node.id, write)
            .await
            .map_err(|err| Error::Command(Box::new(err)))?;

        if let Err(err) = write.ctx.dns.delete(&node.dns_record_id).await {
            warn!("Failed to remove node dns: {err}");
        }

        Ok(())
    }

    /// Finds the next possible host for this node to be tried on.
    pub async fn find_host(
        &self,
        authz: &AuthZ,
        write: &mut WriteConn<'_, '_>,
    ) -> Result<Host, Error> {
        let chain = Blockchain::by_id(self.blockchain_id, authz, write).await?;

        let image = ImageId::new(chain.name, self.node_type, self.version.clone());
        let meta = write.ctx.storage.rhai_metadata(&image).await?;

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
            None => vec![Host::by_id(self.host_id, write).await?],
        };

        // We now have a list of host candidates for our nodes. Now the only thing left to do is to
        // make a decision about where to place the node.
        let deployments = NodeLog::by_node_id(self.id, write).await?;
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

    pub async fn ip(&self, conn: &mut Conn<'_>) -> Result<IpAddress, Error> {
        let ip_addr = self.ip_addr.parse().map_err(Error::ParseIpAddr)?;
        let ip = IpAddress::by_ip(ip_addr, conn).await?;
        Ok(ip)
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

    pub async fn report(
        &self,
        resource: Resource,
        message: String,
        conn: &mut Conn<'_>,
    ) -> Result<NodeReport, Error> {
        let entry = ResourceEntry::from(resource);
        let report = NewNodeReport {
            node_id: self.id,
            created_by: entry.resource_id,
            created_by_resource: entry.resource_type,
            message,
        };
        report.create(conn).await.map_err(Error::Report)
    }

    pub const fn created_by_user(&self) -> Option<UserId> {
        let (Some(resource_type), Some(id)) = (self.created_by_resource, self.created_by) else {
            return None;
        };
        ResourceEntry::new(resource_type, id).user_id()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilteredIpAddr {
    pub ip: String,
    pub description: Option<String>,
}

#[derive(Debug)]
pub struct NodeSearch {
    pub operator: SearchOperator,
    pub id: Option<String>,
    pub name: Option<String>,
    pub ip: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum NodeSort {
    NodeName(SortOrder),
    HostName(SortOrder),
    CreatedAt(SortOrder),
    UpdatedAt(SortOrder),
    NodeType(SortOrder),
    NodeStatus(SortOrder),
    SyncStatus(SortOrder),
    ContainerStatus(SortOrder),
    StakingStatus(SortOrder),
}

impl NodeSort {
    fn into_expr<T>(self) -> Box<dyn BoxableExpression<T, Pg, SqlType = NotSelectable>>
    where
        nodes::name: SelectableExpression<T>,
        hosts::name: SelectableExpression<T>,
        nodes::created_at: SelectableExpression<T>,
        nodes::updated_at: SelectableExpression<T>,
        nodes::node_type: SelectableExpression<T>,
        nodes::node_status: SelectableExpression<T>,
        nodes::container_status: SelectableExpression<T>,
        nodes::sync_status: SelectableExpression<T>,
        nodes::staking_status: SelectableExpression<T>,
    {
        use NodeSort::*;
        use SortOrder::*;

        match self {
            NodeName(Asc) => Box::new(nodes::name.asc()),
            NodeName(Desc) => Box::new(nodes::name.desc()),

            HostName(Asc) => Box::new(hosts::name.asc()),
            HostName(Desc) => Box::new(hosts::name.desc()),

            CreatedAt(Asc) => Box::new(nodes::created_at.asc()),
            CreatedAt(Desc) => Box::new(nodes::created_at.desc()),

            UpdatedAt(Asc) => Box::new(nodes::updated_at.asc()),
            UpdatedAt(Desc) => Box::new(nodes::updated_at.desc()),

            NodeType(Asc) => Box::new(nodes::node_type.asc()),
            NodeType(Desc) => Box::new(nodes::node_type.desc()),

            NodeStatus(Asc) => Box::new(nodes::node_status.asc()),
            NodeStatus(Desc) => Box::new(nodes::node_status.desc()),

            SyncStatus(Asc) => Box::new(nodes::sync_status.asc()),
            SyncStatus(Desc) => Box::new(nodes::sync_status.desc()),

            ContainerStatus(Asc) => Box::new(nodes::container_status.asc()),
            ContainerStatus(Desc) => Box::new(nodes::container_status.desc()),

            StakingStatus(Asc) => Box::new(nodes::staking_status.asc()),
            StakingStatus(Desc) => Box::new(nodes::staking_status.desc()),
        }
    }
}

#[derive(Debug)]
pub struct NodeFilter {
    pub org_ids: Vec<OrgId>,
    pub offset: u64,
    pub limit: u64,
    pub status: Vec<NodeStatus>,
    pub node_types: Vec<NodeType>,
    pub blockchain_ids: Vec<BlockchainId>,
    pub host_ids: Vec<HostId>,
    pub user_ids: Vec<UserId>,
    pub ip_addresses: Vec<String>,
    pub versions: Vec<String>,
    pub networks: Vec<String>,
    pub regions: Vec<String>,
    pub search: Option<NodeSearch>,
    pub sort: VecDeque<NodeSort>,
}

impl NodeFilter {
    pub async fn query(mut self, conn: &mut Conn<'_>) -> Result<(Vec<Node>, u64), Error> {
        let mut query = nodes::table
            .inner_join(hosts::table)
            .left_join(regions::table)
            .into_boxed();

        if let Some(search) = self.search {
            query = query.filter(search.into_expression());
        }

        if !self.org_ids.is_empty() {
            query = query.filter(nodes::org_id.eq_any(self.org_ids));
        }

        if !self.host_ids.is_empty() {
            query = query.filter(nodes::host_id.eq_any(self.host_ids));
        }

        if !self.user_ids.is_empty() {
            query = query.filter(nodes::created_by.eq_any(self.user_ids));
        }

        if !self.blockchain_ids.is_empty() {
            query = query.filter(nodes::blockchain_id.eq_any(self.blockchain_ids));
        }

        if !self.ip_addresses.is_empty() {
            query = query.filter(nodes::ip_addr.eq_any(self.ip_addresses));
        }

        if !self.versions.is_empty() {
            query = query.filter(nodes::version.eq_any(self.versions));
        }

        if !self.networks.is_empty() {
            query = query.filter(nodes::network.eq_any(self.networks));
        }

        if !self.regions.is_empty() {
            query = query.filter(regions::name.eq_any(self.regions));
        }

        // If the user requested a deleted status, we include deleted records with the response.
        // Conversely, if no such request is made, we exlude all deleted rows.
        if !self.status.iter().any(|s| DELETED_STATUSES.contains(s)) {
            query = query.filter(nodes::deleted_at.is_null());
        }
        if !self.status.is_empty() {
            query = query.filter(nodes::node_status.eq_any(self.status));
        }

        if !self.node_types.is_empty() {
            query = query.filter(nodes::node_type.eq_any(self.node_types));
        }

        if let Some(sort) = self.sort.pop_front() {
            query = query.order_by(sort.into_expr());
        } else {
            query = query.order_by(nodes::created_at.desc());
        }

        while let Some(sort) = self.sort.pop_front() {
            query = query.then_order_by(sort.into_expr());
        }

        query
            .select(Node::as_select())
            .paginate(self.limit, self.offset)?
            .count_results(conn)
            .await
            .map_err(Into::into)
    }
}

type NodesHostsAndRegions =
    LeftJoinQuerySource<InnerJoinQuerySource<nodes::table, hosts::table>, regions::table>;

impl NodeSearch {
    fn into_expression(
        self,
    ) -> Box<dyn BoxableExpression<NodesHostsAndRegions, Pg, SqlType = Bool>> {
        match self.operator {
            SearchOperator::Or => {
                let mut predicate: Box<
                    dyn BoxableExpression<NodesHostsAndRegions, Pg, SqlType = Bool>,
                > = Box::new(false.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.or(super::text(nodes::id).like(id)));
                }
                if let Some(name) = self.name {
                    predicate = Box::new(predicate.or(super::lower(nodes::name).like(name)));
                }
                if let Some(ip) = self.ip {
                    predicate = Box::new(predicate.or(nodes::ip_addr.like(ip)));
                }
                predicate
            }
            SearchOperator::And => {
                let mut predicate: Box<
                    dyn BoxableExpression<NodesHostsAndRegions, Pg, SqlType = Bool>,
                > = Box::new(true.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.and(super::text(nodes::id).like(id)));
                }
                if let Some(name) = self.name {
                    predicate = Box::new(predicate.and(super::lower(nodes::name).like(name)));
                }
                if let Some(ip) = self.ip {
                    predicate = Box::new(predicate.and(nodes::ip_addr.like(ip)));
                }
                predicate
            }
        }
    }
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
    pub sync_status: SyncStatus,
    pub staking_status: StakingStatus,
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
        mut self,
        host: Option<Host>,
        authz: &AuthZ,
        mut write: &mut WriteConn<'_, '_>,
    ) -> Result<Node, Error> {
        let host = if let Some(host) = host {
            host
        } else {
            let scheduler = self
                .scheduler(write)
                .await?
                .ok_or(Error::NoHostOrScheduler)?;
            self.find_host(scheduler, authz, write).await?
        };

        let node_ip = IpAddress::next_for_host(host.id, write)
            .await
            .map_err(Error::NextHostIp)?;
        let ip_addr = node_ip.ip().to_string();
        let ip_gateway = host.ip_gateway.ip().to_string();

        node_ip.assign(write).await.map_err(Error::AssignIpAddr)?;
        let dns_record = write.ctx.dns.create(&self.name, node_ip.ip()).await?;
        let dns_id = dns_record.id;

        let blockchain = Blockchain::by_id(self.blockchain_id, authz, write).await?;
        let image = ImageId::new(blockchain.name, self.node_type, self.version.clone());
        let meta = write.ctx.storage.rhai_metadata(&image).await?;
        let data_directory_mountpoint = meta
            .babel_config
            .and_then(|cfg| cfg.data_directory_mount_point);

        Org::increment_node(self.org_id, write).await?;
        Host::increment_node(host.id, write).await?;

        loop {
            match diesel::insert_into(nodes::table)
                .values((
                    &self,
                    nodes::host_id.eq(host.id),
                    nodes::ip_gateway.eq(&ip_gateway),
                    nodes::ip_addr.eq(&ip_addr),
                    nodes::dns_record_id.eq(&dns_id),
                    nodes::data_directory_mountpoint.eq(&data_directory_mountpoint),
                    nodes::url.eq(&dns_record.name),
                ))
                .get_result(&mut write)
                .await
            {
                Ok(node) => return Ok(node),
                Err(DatabaseError(UniqueViolation, info)) if info.column_name() == Some("name") => {
                    warn!("Node name {} already taken. Retrying...", self.name);
                    self.name = Petnames::small()
                        .generate_one(3, "-")
                        .ok_or(Error::RegenerateName)?;
                    continue;
                }
                Err(err) => {
                    if let Err(err) = node_ip.unassign(write).await {
                        warn!("Failed to unassign IP {node_ip:?} for aborted node creation: {err}");
                    }
                    if let Err(err) = write.ctx.dns.delete(&dns_id).await {
                        warn!(
                            "Failed to delete DNS record {dns_id} for aborted node creation: {err}"
                        );
                    }
                    return Err(Error::Create(err));
                }
            }
        }
    }

    /// Finds the most suitable host to initially place the node on.
    ///
    /// Since this is a freshly created node, we do not need to worry about
    /// logic regarding where to retry placing the node. We simply ask for an
    /// ordered list of the most suitable hosts, and pick the first one.
    async fn find_host(
        &self,
        scheduler: NodeScheduler,
        authz: &AuthZ,
        write: &mut WriteConn<'_, '_>,
    ) -> Result<Host, Error> {
        let chain = Blockchain::by_id(self.blockchain_id, authz, write).await?;

        let image = ImageId::new(chain.name, self.node_type, self.version.clone());
        let metadata = write.ctx.storage.rhai_metadata(&image).await?;

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

#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = nodes)]
pub struct UpdateNode<'a> {
    pub org_id: Option<OrgId>,
    pub host_id: Option<HostId>,
    pub name: Option<&'a str>,
    pub version: Option<NodeVersion>,
    pub ip_addr: Option<&'a str>,
    pub ip_gateway: Option<&'a str>,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub node_status: Option<NodeStatus>,
    pub sync_status: Option<SyncStatus>,
    pub staking_status: Option<StakingStatus>,
    pub container_status: Option<ContainerStatus>,
    pub self_update: Option<bool>,
    pub address: Option<&'a str>,
    pub allow_ips: Option<serde_json::Value>,
    pub deny_ips: Option<serde_json::Value>,
    pub note: Option<&'a str>,
}

/// Update node columns related to metrics.
#[derive(Debug, Insertable, AsChangeset)]
#[diesel(table_name = nodes)]
pub struct UpdateNodeMetrics {
    pub id: NodeId,
    pub block_height: Option<i64>,
    pub block_age: Option<i64>,
    pub staking_status: Option<StakingStatus>,
    pub consensus: Option<bool>,
    pub node_status: Option<NodeStatus>,
    pub sync_status: Option<SyncStatus>,
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

    use crate::auth::rbac::access::tests::view_authz;
    use crate::config::Context;

    use super::*;

    #[tokio::test]
    async fn can_filter_nodes() {
        let (ctx, db) = Context::with_mocked().await.unwrap();

        let blockchain_id = db.seed.blockchain.id;
        let user_id = db.seed.user.id;
        let org_id = db.seed.org.id;

        let req = NewNode {
            id: Uuid::new_v4().into(),
            org_id,
            blockchain_id,
            node_status: NodeStatus::Ingesting,
            sync_status: SyncStatus::Syncing,
            container_status: ContainerStatus::Installing,
            block_height: None,
            node_data: None,
            name: "my-test-node".to_string(),
            version: "3.3.0".to_string().into(),
            staking_status: StakingStatus::Staked,
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

        let authz = view_authz(&ctx, db.seed.node.id, &mut write).await;
        let host = db.seed.host.clone();
        let host_id = db.seed.host.id;
        req.create(Some(host), &authz, &mut write).await.unwrap();

        let filter = NodeFilter {
            org_ids: vec![org_id],
            offset: 0,
            limit: 10,
            status: vec![NodeStatus::Ingesting],
            node_types: vec![],
            blockchain_ids: vec![blockchain_id],
            host_ids: vec![host_id],
            user_ids: vec![],
            ip_addresses: vec![],
            versions: vec![],
            networks: vec![],
            regions: vec![],
            search: None,
            sort: VecDeque::new(),
        };

        let (nodes, _count) = filter.query(&mut write).await.unwrap();
        assert_eq!(nodes.len(), 1);
    }
}
