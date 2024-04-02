use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Utc};
use diesel::dsl;
use diesel::expression::expression_types::NotSelectable;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel::sql_types::Bool;
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display;
use ipnetwork::IpNetwork;
use thiserror::Error;
use tonic::Status;

use crate::auth::rbac::HostBillingPerm;
use crate::auth::resource::{HostId, OrgId, UserId};
use crate::auth::AuthZ;
use crate::database::Conn;
use crate::grpc::{api, common};
use crate::storage::metadata::HardwareRequirements;
use crate::util::{SearchOperator, SortOrder};

use super::blockchain::{Blockchain, BlockchainId};
use super::ip_address::CreateIpAddress;
use super::node::{NodeScheduler, NodeType, ResourceAffinity};
use super::schema::{hosts, sql_types};
use super::{Command, Org, Paginate, Region, RegionId};

type NotDeleted = dsl::Filter<hosts::table, dsl::IsNull<hosts::deleted_at>>;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Protobuf BillingAmount is missing an Amount.
    BillingAmountMissingAmount,
    /// Unsupported BillingAmount Currency: {0:?}
    BillingAmountCurrency(i32),
    /// Unsupported BillingAmount Period: {0:?}
    BillingAmountPeriod(i32),
    /// Host Command error: {0}
    Command(Box<super::command::Error>),
    /// Failed to create host: {0}
    Create(diesel::result::Error),
    /// Failed to decrement node count for host `{0}`: {1}
    DecrementNode(HostId, diesel::result::Error),
    /// Failed to delete host id `{0}`: {1}
    Delete(HostId, diesel::result::Error),
    /// Failed to find host by id `{0}`: {1}
    FindById(HostId, diesel::result::Error),
    /// Failed to find hosts by id `{0:?}`: {1}
    FindByIds(HashSet<HostId>, diesel::result::Error),
    /// Failed to find host ids `{0:?}`: {1}
    FindExistingIds(HashSet<HostId>, diesel::result::Error),
    /// Failed to find host by name `{0}`: {1}
    FindByName(String, diesel::result::Error),
    /// Failed to get host candidates: {0}
    HostCandidates(diesel::result::Error),
    /// Failed to increment node count for host `{0}`: {1}
    IncrementNode(HostId, diesel::result::Error),
    /// Host ip address error: {0}
    IpAddress(#[from] crate::models::ip_address::Error),
    /// Failed to get node counts for host: {0}
    NodeCounts(diesel::result::Error),
    /// Nothing to update.
    NoUpdate,
    /// Host org error: {0}
    Org(#[from] crate::models::org::Error),
    /// Host pagination: {0}
    Paginate(#[from] crate::models::paginate::Error),
    /// Failed to parse host ip address: {0}
    ParseIp(std::net::AddrParseError),
    /// Host region error: {0}
    Region(super::region::Error),
    /// Failed to update host: {0}
    Update(diesel::result::Error),
    /// Failed to update host {1}'s metrics: {0}
    UpdateMetrics(diesel::result::Error, HostId),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            Delete(_, NotFound)
            | FindById(_, NotFound)
            | FindByIds(_, NotFound)
            | FindByName(_, NotFound) => Status::not_found("Not found."),
            BillingAmountMissingAmount | BillingAmountCurrency(_) | BillingAmountPeriod(_) => {
                Status::invalid_argument("billing_amount")
            }
            NoUpdate => Status::failed_precondition("Nothing to update."),
            ParseIp(_) => Status::invalid_argument("ip_addr"),
            Paginate(err) => err.into(),
            IpAddress(err) => err.into(),
            Org(err) => err.into(),
            Region(err) => err.into(),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumHostType"]
pub enum HostType {
    /// Anyone can run nodes on these servers.
    Cloud,
    /// Only people in the org can run nodes on these servers. They are for private use.
    Private,
}

#[derive(Debug, Clone, Queryable)]
#[diesel(table_name = hosts)]
pub struct Host {
    pub id: HostId,
    pub version: String,
    pub name: String,
    pub ip_addr: String,
    pub status: ConnectionStatus,
    pub created_at: DateTime<Utc>,
    // Number of CPU's that this host has.
    pub cpu_count: i64,
    // The size of the hosts memory, in bytes.
    pub mem_size_bytes: i64,
    // The size of the hosts disk, in bytes.
    pub disk_size_bytes: i64,
    pub os: String,
    pub os_version: String,
    pub ip_gateway: IpNetwork,
    pub used_cpu: Option<i32>,
    pub used_memory: Option<i64>,
    pub used_disk_space: Option<i64>,
    pub load_one: Option<f64>,
    pub load_five: Option<f64>,
    pub load_fifteen: Option<f64>,
    pub network_received: Option<i64>,
    pub network_sent: Option<i64>,
    pub uptime: Option<i64>,
    pub host_type: Option<HostType>,
    /// The id of the org that owns and operates this host.
    pub org_id: OrgId,
    /// This is the id of the user that created this host. For older hosts, this value might not be
    /// set.
    pub created_by: Option<UserId>,
    // The id of the region where this host is located.
    pub region_id: Option<RegionId>,
    // The monthly billing amount for this host (only visible to host owners).
    pub monthly_cost_in_usd: Option<MonthlyCostUsd>,
    pub vmm_mountpoint: Option<String>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub managed_by: ManagedBy,
    pub node_count: i32,
}

impl AsRef<Host> for Host {
    fn as_ref(&self) -> &Host {
        self
    }
}

#[derive(Debug)]
pub struct HostRequirements {
    pub requirements: HardwareRequirements,
    pub blockchain_id: BlockchainId,
    pub node_type: NodeType,
    pub host_type: Option<HostType>,
    pub scheduler: NodeScheduler,
    pub org_id: Option<OrgId>,
}

impl Host {
    pub async fn by_id(id: HostId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        hosts::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn by_ids(ids: HashSet<HostId>, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        hosts::table
            .filter(hosts::id.eq_any(ids.iter()))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByIds(ids, err))
    }

    /// Filters out any node ids that do no exist.
    pub async fn existing_ids(
        ids: HashSet<HostId>,
        conn: &mut Conn<'_>,
    ) -> Result<HashSet<HostId>, Error> {
        let ids = Self::not_deleted()
            .filter(hosts::id.eq_any(ids.iter()))
            .select(hosts::id)
            .get_results(conn)
            .await
            .map_err(|err| Error::FindExistingIds(ids, err))?;
        Ok(ids.into_iter().collect())
    }

    pub async fn by_name(name: &str, conn: &mut Conn<'_>) -> Result<Self, Error> {
        Self::not_deleted()
            .filter(hosts::name.eq(name))
            .get_result(conn)
            .await
            .map_err(|err| Error::FindByName(name.into(), err))
    }

    pub async fn increment_node(host_id: HostId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(hosts::table.filter(hosts::id.eq(host_id)))
            .set(hosts::node_count.eq(hosts::node_count + 1))
            .get_result(conn)
            .await
            .map_err(|err| Error::IncrementNode(host_id, err))
    }

    pub async fn decrement_node(host_id: HostId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(hosts::table.filter(hosts::id.eq(host_id)))
            .set(hosts::node_count.eq(hosts::node_count - 1))
            .get_result(conn)
            .await
            .map_err(|err| Error::DecrementNode(host_id, err))
    }

    pub async fn delete(id: HostId, conn: &mut Conn<'_>) -> Result<(), Error> {
        let host: Host = diesel::update(Self::not_deleted().find(id))
            .set(hosts::deleted_at.eq(Utc::now()))
            .get_result(conn)
            .await
            .map_err(|err| Error::Delete(id, err))?;

        Org::decrement_host(host.org_id, conn).await?;
        Command::delete_host_pending(id, conn)
            .await
            .map_err(|err| Error::Command(Box::new(err)))?;

        Ok(())
    }

    /// This function returns a list of up to 2 possible hosts that the node may be scheduled on.
    /// This list is ordered by suitability, the best fit will be first in the list. Note that zero
    /// hosts may be returned when our system is out of resources, and this case should be handled
    /// gracefully.
    pub async fn host_candidates(
        reqs: HostRequirements,
        limit: Option<i64>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Host>, Error> {
        use diesel::sql_types::{BigInt, Nullable, Uuid};
        use sql_types::EnumNodeType;

        #[derive(Debug, QueryableByName)]
        struct HostCandidate {
            #[diesel(sql_type = Uuid)]
            host_id: HostId,
        }

        let HostRequirements {
            requirements,
            blockchain_id,
            node_type,
            host_type,
            scheduler,
            org_id,
        } = reqs;

        let order_by = scheduler.order_clause();
        let limit_clause = limit.map(|_| "LIMIT $6").unwrap_or_default();
        let region_clause = scheduler
            .region
            .as_ref()
            .map(|_| "AND region_id = $7")
            .unwrap_or_default();
        let org_clause = org_id.map(|_| "AND org_id = $8").unwrap_or_default();
        let host_type_clause = host_type.map(|_| "AND host_type = $9").unwrap_or_default();

        // SAFETY: We are using `format!` to place a custom generated ORDER BY clause into a sql
        // query. This is injection-safe because the clause is entirely generated from static
        // strings, not user input.
        let query = format!("
        SELECT
            host_id
        FROM
        (
            SELECT
                id as host_id,
                hosts.cpu_count - (SELECT COALESCE(SUM(vcpu_count), 0)::BIGINT FROM nodes WHERE deleted_at IS NULL AND host_id = hosts.id) AS av_cpus,
                hosts.mem_size_bytes - (SELECT COALESCE(SUM(mem_size_bytes), 0)::BIGINT FROM nodes WHERE deleted_at IS NULL AND host_id = hosts.id) AS av_mem,
                hosts.disk_size_bytes - (SELECT COALESCE(SUM(disk_size_bytes), 0)::BIGINT FROM nodes WHERE deleted_at IS NULL AND host_id = hosts.id) AS av_disk,
                (SELECT COUNT(*) FROM ip_addresses WHERE ip_addresses.host_id = hosts.id AND NOT ip_addresses.is_assigned) AS ips,
                (SELECT COUNT(*) FROM nodes WHERE deleted_at IS NULL AND host_id = hosts.id AND blockchain_id = $4 AND node_type = $5 AND host_type = 'cloud') AS n_similar,
                hosts.region_id AS region_id,
                hosts.org_id AS org_id,
                hosts.host_type AS host_type
            FROM
                hosts
            WHERE
                deleted_at IS NULL AND
                managed_by = 'automatic'
        ) AS resouces
        WHERE
            -- These are our hard filters, we do not want any nodes that cannot satisfy the
            -- requirements or are in the wrong region
            av_cpus > $1 AND
            av_mem > $2 AND
            av_disk > $3 AND
            ips > 0
        {region_clause}
        {org_clause}
        {host_type_clause}
        {order_by}
        {limit_clause};");

        #[allow(clippy::cast_possible_wrap)]
        let hosts: Vec<HostCandidate> = diesel::sql_query(query)
            .bind::<BigInt, _>(i64::from(requirements.vcpu_count))
            .bind::<BigInt, _>(requirements.mem_size_mb as i64 * 1000 * 1000)
            .bind::<BigInt, _>(requirements.disk_size_gb as i64 * 1000 * 1000 * 1000)
            .bind::<Uuid, _>(blockchain_id)
            .bind::<EnumNodeType, _>(node_type)
            .bind::<Nullable<BigInt>, _>(limit)
            .bind::<Nullable<Uuid>, _>(scheduler.region.map(|r| r.id))
            .bind::<Nullable<Uuid>, _>(org_id)
            .bind::<Nullable<sql_types::EnumHostType>, _>(host_type)
            .get_results(conn)
            .await
            .map_err(Error::HostCandidates)?;
        let host_ids = hosts.into_iter().map(|h| h.host_id).collect();

        Self::by_ids(host_ids, conn).await
    }

    pub async fn regions_for(
        org_id: impl Into<Option<OrgId>> + Send,
        blockchain: Blockchain,
        node_type: NodeType,
        requirements: HardwareRequirements,
        host_type: Option<HostType>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Region>, Error> {
        let scheduler = NodeScheduler {
            region: None,
            similarity: None,
            resource: ResourceAffinity::LeastResources,
        };
        let org_id = (host_type == Some(HostType::Private))
            .then(|| org_id.into())
            .flatten();
        let requirements = HostRequirements {
            requirements,
            blockchain_id: blockchain.id,
            node_type,
            host_type,
            scheduler,
            org_id,
        };
        let regions = Self::host_candidates(requirements, None, conn)
            .await?
            .into_iter()
            .filter_map(|host| host.region_id)
            .collect();

        Region::by_ids(regions, conn).await.map_err(Error::Region)
    }

    /// Extract the monthly cost for external display.
    pub fn monthly_cost_in_usd(&self, authz: &AuthZ) -> Option<i64> {
        if let Some(MonthlyCostUsd(cost)) = self.monthly_cost_in_usd {
            authz.has_perm(HostBillingPerm::Get).then_some(cost)
        } else {
            None
        }
    }

    pub fn not_deleted() -> NotDeleted {
        hosts::table.filter(hosts::deleted_at.is_null())
    }
}

#[derive(Debug)]
pub struct HostSearch {
    pub operator: SearchOperator,
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub os: Option<String>,
    pub ip: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum HostSort {
    HostName(SortOrder),
    CreatedAt(SortOrder),
    Version(SortOrder),
    Os(SortOrder),
    OsVersion(SortOrder),
    CpuCount(SortOrder),
    MemSizeBytes(SortOrder),
    DiskSizeBytes(SortOrder),
    NodeCount(SortOrder),
}

impl HostSort {
    fn into_expr<T>(self) -> Box<dyn BoxableExpression<T, Pg, SqlType = NotSelectable>>
    where
        hosts::name: SelectableExpression<T>,
        hosts::created_at: SelectableExpression<T>,
        hosts::version: SelectableExpression<T>,
        hosts::os: SelectableExpression<T>,
        hosts::os_version: SelectableExpression<T>,
        hosts::cpu_count: SelectableExpression<T>,
        hosts::mem_size_bytes: SelectableExpression<T>,
        hosts::disk_size_bytes: SelectableExpression<T>,
        hosts::node_count: SelectableExpression<T>,
    {
        use HostSort::*;
        use SortOrder::*;

        match self {
            HostName(Asc) => Box::new(hosts::name.asc()),
            HostName(Desc) => Box::new(hosts::name.desc()),

            CreatedAt(Asc) => Box::new(hosts::created_at.asc()),
            CreatedAt(Desc) => Box::new(hosts::created_at.desc()),

            Version(Asc) => Box::new(hosts::version.asc()),
            Version(Desc) => Box::new(hosts::version.desc()),

            Os(Asc) => Box::new(hosts::os.asc()),
            Os(Desc) => Box::new(hosts::os.desc()),

            OsVersion(Asc) => Box::new(hosts::os_version.asc()),
            OsVersion(Desc) => Box::new(hosts::os_version.desc()),

            CpuCount(Asc) => Box::new(hosts::cpu_count.asc()),
            CpuCount(Desc) => Box::new(hosts::cpu_count.desc()),

            MemSizeBytes(Asc) => Box::new(hosts::mem_size_bytes.asc()),
            MemSizeBytes(Desc) => Box::new(hosts::mem_size_bytes.desc()),

            DiskSizeBytes(Asc) => Box::new(hosts::disk_size_bytes.asc()),
            DiskSizeBytes(Desc) => Box::new(hosts::disk_size_bytes.desc()),

            NodeCount(Asc) => Box::new(hosts::node_count.asc()),
            NodeCount(Desc) => Box::new(hosts::node_count.desc()),
        }
    }
}

#[derive(Debug)]
pub struct HostFilter {
    pub org_id: Option<OrgId>,
    pub offset: u64,
    pub limit: u64,
    pub search: Option<HostSearch>,
    pub sort: VecDeque<HostSort>,
}

impl HostFilter {
    pub async fn query(mut self, conn: &mut Conn<'_>) -> Result<(Vec<Host>, u64), Error> {
        let mut query = Host::not_deleted().into_boxed();

        if let Some(search) = self.search {
            query = query.filter(search.into_expression());
        }

        if let Some(org_id) = self.org_id {
            query = query.filter(hosts::org_id.eq(org_id));
        }

        if let Some(sort) = self.sort.pop_front() {
            query = query.order_by(sort.into_expr());
        } else {
            query = query.order_by(hosts::created_at.desc());
        }

        while let Some(sort) = self.sort.pop_front() {
            query = query.then_order_by(sort.into_expr());
        }

        query
            .paginate(self.limit, self.offset)?
            .count_results(conn)
            .await
            .map_err(Into::into)
    }
}

impl HostSearch {
    fn into_expression(self) -> Box<dyn BoxableExpression<hosts::table, Pg, SqlType = Bool>> {
        match self.operator {
            SearchOperator::Or => {
                let mut predicate: Box<dyn BoxableExpression<hosts::table, Pg, SqlType = Bool>> =
                    Box::new(false.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.or(super::text(hosts::id).like(id)));
                }
                if let Some(name) = self.name {
                    predicate = Box::new(predicate.or(super::lower(hosts::name).like(name)));
                }
                if let Some(version) = self.version {
                    predicate = Box::new(predicate.or(super::lower(hosts::version).like(version)));
                }
                if let Some(os) = self.os {
                    predicate = Box::new(predicate.or(super::lower(hosts::os).like(os)));
                }
                if let Some(ip) = self.ip {
                    predicate = Box::new(predicate.or(hosts::ip_addr.like(ip)));
                }
                predicate
            }
            SearchOperator::And => {
                let mut predicate: Box<dyn BoxableExpression<hosts::table, Pg, SqlType = Bool>> =
                    Box::new(true.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.and(super::text(hosts::id).like(id)));
                }
                if let Some(name) = self.name {
                    predicate = Box::new(predicate.and(super::lower(hosts::name).like(name)));
                }
                if let Some(version) = self.version {
                    predicate = Box::new(predicate.and(super::lower(hosts::version).like(version)));
                }
                if let Some(os) = self.os {
                    predicate = Box::new(predicate.and(super::lower(hosts::os).like(os)));
                }
                if let Some(ip) = self.ip {
                    predicate = Box::new(predicate.and(hosts::ip_addr.like(ip)));
                }
                predicate
            }
        }
    }
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = hosts)]
pub struct NewHost<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub cpu_count: i64,
    /// The amount of memory in bytes that this host has.
    pub mem_size_bytes: i64,
    /// The amount of disk space in bytes that this host has.
    pub disk_size_bytes: i64,
    pub os: &'a str,
    pub os_version: &'a str,
    pub ip_addr: &'a str,
    pub status: ConnectionStatus,
    pub ip_gateway: IpNetwork,
    /// The id of the org that owns and operates this host.
    pub org_id: OrgId,
    /// This is the id of the user that created this host.
    pub created_by: UserId,
    // The id of the region where this host is located.
    pub region_id: Option<RegionId>,
    pub host_type: HostType,
    pub monthly_cost_in_usd: Option<MonthlyCostUsd>,
    pub vmm_mountpoint: Option<&'a str>,
    pub managed_by: ManagedBy,
}

impl NewHost<'_> {
    /// Creates a new `Host` in the db, including the necessary related rows.
    pub async fn create(self, ips: &[IpNetwork], conn: &mut Conn<'_>) -> Result<Host, Error> {
        let host: Host = diesel::insert_into(hosts::table)
            .values(&self)
            .get_result(conn)
            .await
            .map_err(Error::Create)?;
        Org::increment_host(self.org_id, conn).await?;
        let new_ips: Vec<_> = ips
            .iter()
            .map(|&ip| CreateIpAddress::new(ip, host.id))
            .collect();
        CreateIpAddress::bulk_create(new_ips, conn).await?;
        Ok(host)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, AsChangeset)]
#[diesel(table_name = hosts)]
pub struct UpdateHost<'a> {
    pub id: HostId,
    pub name: Option<&'a str>,
    pub version: Option<&'a str>,
    pub cpu_count: Option<i64>,
    pub mem_size_bytes: Option<i64>,
    pub disk_size_bytes: Option<i64>,
    pub os: Option<&'a str>,
    pub os_version: Option<&'a str>,
    pub ip_addr: Option<&'a str>,
    pub status: Option<ConnectionStatus>,
    pub ip_gateway: Option<IpNetwork>,
    pub region_id: Option<RegionId>,
    pub managed_by: Option<ManagedBy>,
}

impl UpdateHost<'_> {
    pub const fn new(id: HostId) -> Self {
        UpdateHost {
            id,
            name: None,
            version: None,
            cpu_count: None,
            mem_size_bytes: None,
            disk_size_bytes: None,
            os: None,
            os_version: None,
            ip_addr: None,
            status: None,
            ip_gateway: None,
            region_id: None,
            managed_by: None,
        }
    }

    #[must_use]
    pub const fn with_status(mut self, status: ConnectionStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub async fn update(self, conn: &mut Conn<'_>) -> Result<Host, Error> {
        if self == Self::new(self.id) {
            return Err(Error::NoUpdate);
        }

        diesel::update(Host::not_deleted().find(self.id))
            .set(self)
            .get_result(conn)
            .await
            .map_err(Error::Update)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = hosts)]
pub struct UpdateHostMetrics {
    pub id: HostId,
    pub used_cpu: Option<i32>,
    pub used_memory: Option<i64>,
    pub used_disk_space: Option<i64>,
    pub load_one: Option<f64>,
    pub load_five: Option<f64>,
    pub load_fifteen: Option<f64>,
    pub network_received: Option<i64>,
    pub network_sent: Option<i64>,
    pub uptime: Option<i64>,
}

impl UpdateHostMetrics {
    /// Performs a selective update of only the columns related to metrics of the provided nodes.
    pub async fn update_metrics(
        mut updates: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Host>, Error> {
        // We do this for determinism in our tests.
        updates.sort_by_key(|u| u.id);

        let mut hosts = Vec::with_capacity(updates.len());
        for update in updates {
            let updated = diesel::update(Host::not_deleted().find(update.id))
                .set(&update)
                .get_result(conn)
                .await
                .map_err(|err| Error::UpdateMetrics(err, update.id))?;
            hosts.push(updated);
        }
        Ok(hosts)
    }
}

/// The billing cost per month in USD for this host.
///
/// The inner cost is extracted via `Host::monthly_cost_in_usd`.
#[derive(Clone, Copy, Debug, DieselNewType)]
pub struct MonthlyCostUsd(i64);

impl MonthlyCostUsd {
    pub fn from_proto(billing: &common::BillingAmount) -> Result<Self, Error> {
        let amount = match billing.amount {
            Some(ref amount) => Ok(amount),
            None => Err(Error::BillingAmountMissingAmount),
        }?;

        match common::Currency::try_from(amount.currency) {
            Ok(common::Currency::Usd) => Ok(()),
            _ => Err(Error::BillingAmountCurrency(amount.currency)),
        }?;

        match common::Period::try_from(billing.period) {
            Ok(common::Period::Monthly) => Ok(()),
            _ => Err(Error::BillingAmountPeriod(billing.period)),
        }?;

        Ok(MonthlyCostUsd(amount.value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumConnStatus"]
pub enum ConnectionStatus {
    Online,
    Offline,
}

impl From<api::HostConnectionStatus> for ConnectionStatus {
    fn from(status: api::HostConnectionStatus) -> Self {
        match status {
            api::HostConnectionStatus::Unspecified | api::HostConnectionStatus::Offline => {
                ConnectionStatus::Offline
            }
            api::HostConnectionStatus::Online => ConnectionStatus::Online,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumManagedBy"]
pub enum ManagedBy {
    Automatic,
    Manual,
}

impl From<api::ManagedBy> for Option<ManagedBy> {
    fn from(managed_by: api::ManagedBy) -> Self {
        match managed_by {
            api::ManagedBy::Unspecified => None,
            api::ManagedBy::Automatic => Some(ManagedBy::Automatic),
            api::ManagedBy::Manual => Some(ManagedBy::Manual),
        }
    }
}
