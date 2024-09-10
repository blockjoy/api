use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Utc};
use diesel::dsl::{count, exists, not, sql};
use diesel::expression::expression_types::NotSelectable;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel::sql_types::{Bool, Nullable};
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use displaydoc::Display;
use thiserror::Error;

use crate::auth::resource::{HostId, OrgId, Resource, ResourceId, ResourceType};
use crate::database::Conn;
use crate::grpc::api;
use crate::util::sql::{self, greatest, IpNetwork, Tags, Version};
use crate::util::{SearchOperator, SortOrder};

use super::ip_address::CreateIpAddress;
use super::node::{NodeScheduler, ResourceAffinity, SimilarNodeAffinity};
use super::schema::{hosts, ip_addresses, nodes, sql_types};
use super::{Command, Node, Org, Paginate, Protocol, RegionId};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to increment node count for host `{0}`: {1}
    AddNode(HostId, diesel::result::Error),
    /// Protobuf BillingAmount is missing an Amount.
    BillingMissingAmount,
    /// Unknown BillingAmount Currency.
    BillingCurrencyUnknown,
    /// Unknown BillingAmount Period.
    BillingPeriodUnknown,
    /// Host Command error: {0}
    Command(Box<super::command::Error>),
    /// Failed to parse cpu cores as i64: {0}
    CpuCores(std::num::TryFromIntError),
    /// Failed to create host: {0}
    Create(diesel::result::Error),
    /// Failed to delete host id `{0}`: {1}
    Delete(HostId, diesel::result::Error),
    /// Failed to parse disk_bytes as i64: {0}
    DiskBytes(std::num::TryFromIntError),
    /// Failed to find host by id `{0}`: {1}
    FindById(HostId, diesel::result::Error),
    /// Failed to find hosts by id `{0:?}`: {1}
    FindByIds(HashSet<HostId>, diesel::result::Error),
    /// Failed to find org id for host id `{0}`: {1}
    FindOrgId(HostId, diesel::result::Error),
    /// Failed to get host candidates: {0}
    HostCandidates(diesel::result::Error),
    /// Host ip address error: {0}
    IpAddress(#[from] crate::model::ip_address::Error),
    /// Failed to parse mem_bytes as i64: {0}
    MemoryBytes(std::num::TryFromIntError),
    /// Failed to get node counts for host: {0}
    NodeCounts(diesel::result::Error),
    /// Nothing to update.
    NoUpdate,
    /// Host org error: {0}
    Org(#[from] crate::model::org::Error),
    /// Host pagination: {0}
    Paginate(#[from] crate::model::paginate::Error),
    /// Failed to parse host ip address: {0}
    ParseIp(std::net::AddrParseError),
    /// Failed to decrement node count for host `{0}`: {1}
    RemoveNode(HostId, diesel::result::Error),
    /// Unknown ConnectionStatus.
    UnknownConnectionStatus,
    /// Unknown ScheduleType.
    UnknownScheduleType,
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
            | FindOrgId(_, NotFound) => Status::not_found("Not found."),
            BillingMissingAmount | BillingCurrencyUnknown | BillingPeriodUnknown => {
                Status::invalid_argument("billing_amount")
            }
            CpuCores(_) => Status::invalid_argument("cpu_cores"),
            DiskBytes(_) => Status::invalid_argument("disk_bytes"),
            MemoryBytes(_) => Status::invalid_argument("memory_bytes"),
            NoUpdate => Status::failed_precondition("Nothing to update."),
            ParseIp(_) => Status::invalid_argument("ip_addr"),
            UnknownConnectionStatus => Status::invalid_argument("connection_status"),
            UnknownScheduleType => Status::invalid_argument("schedule_type"),
            Paginate(err) => err.into(),
            IpAddress(err) => err.into(),
            Org(err) => err.into(),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Debug, Queryable)]
#[diesel(table_name = hosts)]
pub struct Host {
    pub id: HostId,
    pub org_id: Option<OrgId>,
    pub region_id: Option<RegionId>,
    pub network_name: String,
    pub display_name: Option<String>,
    pub schedule_type: ScheduleType,
    pub connection_status: ConnectionStatus,
    pub cpu_cores: i64,
    pub memory_bytes: i64,
    pub disk_bytes: i64,
    pub os: String,
    pub os_version: String,
    pub bv_version: Version,
    pub ip_address: IpNetwork,
    pub ip_gateway: IpNetwork,
    pub node_count: i64,
    pub node_cpu_cores: i64,
    pub node_memory_bytes: i64,
    pub node_disk_bytes: i64,
    pub used_cpu_hundreths: Option<i64>,
    pub used_memory_bytes: Option<i64>,
    pub used_disk_bytes: Option<i64>,
    pub load_one_percent: Option<f64>,
    pub load_five_percent: Option<f64>,
    pub load_fifteen_percent: Option<f64>,
    pub network_received_bytes: Option<i64>,
    pub network_sent_bytes: Option<i64>,
    pub uptime_seconds: Option<i64>,
    pub tags: Tags,
    pub created_by_type: ResourceType,
    pub created_by_id: ResourceId,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl AsRef<Host> for Host {
    fn as_ref(&self) -> &Host {
        self
    }
}

impl Host {
    pub async fn by_id(
        id: HostId,
        org_id: Option<OrgId>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        hosts::table
            .find(id)
            .filter(hosts::org_id.eq(org_id).or(hosts::org_id.is_null()))
            .filter(hosts::deleted_at.is_null())
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn by_ids(
        ids: &HashSet<HostId>,
        org_ids: &HashSet<OrgId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        hosts::table
            .filter(hosts::id.eq_any(ids))
            .filter(hosts::org_id.eq_any(org_ids).or(hosts::org_id.is_null()))
            .filter(hosts::deleted_at.is_null())
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByIds(ids.clone(), err))
    }

    pub async fn org_id(id: HostId, conn: &mut Conn<'_>) -> Result<Option<OrgId>, Error> {
        hosts::table
            .find(id)
            .filter(hosts::deleted_at.is_null())
            .select(hosts::org_id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindOrgId(id, err))
    }

    pub async fn add_node(node: &Node, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(hosts::table.filter(hosts::id.eq(node.host_id)))
            .set((
                hosts::node_count.eq(hosts::node_count + 1),
                hosts::node_cpu_cores.eq(hosts::node_cpu_cores + node.cpu_cores),
                hosts::node_memory_bytes.eq(hosts::node_memory_bytes + node.memory_bytes),
                hosts::node_disk_bytes.eq(hosts::node_disk_bytes + node.disk_bytes),
            ))
            .get_result(conn)
            .await
            .map_err(|err| Error::AddNode(node.host_id, err))
    }

    pub async fn remove_node(node: &Node, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(hosts::table.filter(hosts::id.eq(node.host_id)))
            .set((
                hosts::node_count.eq(greatest(0, hosts::node_count - 1)),
                hosts::node_cpu_cores.eq(greatest(0, hosts::node_cpu_cores - node.cpu_cores)),
                hosts::node_memory_bytes
                    .eq(greatest(0, hosts::node_memory_bytes - node.memory_bytes)),
                hosts::node_disk_bytes.eq(greatest(0, hosts::node_disk_bytes - node.disk_bytes)),
            ))
            .get_result(conn)
            .await
            .map_err(|err| Error::RemoveNode(node.host_id, err))
    }

    pub async fn delete(
        id: HostId,
        org_id: Option<OrgId>,
        conn: &mut Conn<'_>,
    ) -> Result<(), Error> {
        let row = hosts::table
            .find(id)
            .filter(hosts::org_id.eq(org_id).or(hosts::org_id.is_null()))
            .filter(hosts::deleted_at.is_null());
        diesel::update(row)
            .set(hosts::deleted_at.eq(Utc::now()))
            .execute(conn)
            .await
            .map_err(|err| Error::Delete(id, err))?;

        if let Some(org_id) = org_id {
            Org::remove_host(org_id, conn).await?;
        }

        Command::delete_host_pending(id, conn)
            .await
            .map_err(|err| Error::Command(Box::new(err)))?;

        Ok(())
    }

    /// List suitable hosts for a node to be scheduled on.
    pub async fn candidates<'r>(
        require: HostRequirements<'r>,
        limit: Option<i64>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Host>, Error> {
        let free_cpu = hosts::cpu_cores - hosts::node_cpu_cores;
        let free_memory = hosts::memory_bytes - hosts::node_memory_bytes;
        let free_disk = hosts::disk_bytes - hosts::node_disk_bytes;
        let free_ips = ip_addresses::table
            .filter(ip_addresses::host_id.eq(hosts::id))
            .filter(not(exists(
                nodes::table
                    .filter(nodes::ip_address.eq(ip_addresses::ip))
                    .filter(nodes::deleted_at.is_null())
                    .select(nodes::id),
            )))
            .select(count(ip_addresses::id))
            .single_value();

        // type constructor ensures injection safety
        let tag = &require.protocol.key;
        let tag_filter = format!("'{tag}' = ANY(tags) OR CARDINALITY(tags) = 0");
        let tag_order = format!("'{tag}' = ANY(tags)");

        let mut query = hosts::table
            .filter(hosts::deleted_at.is_null())
            .filter(hosts::schedule_type.eq(ScheduleType::Automatic))
            .filter(free_cpu.gt(require.cpu_cores))
            .filter(free_memory.gt(require.memory_bytes))
            .filter(free_disk.gt(require.disk_bytes))
            .filter(free_ips.gt(0))
            .filter(sql::<Bool>(&tag_filter))
            .order_by(sql::<Bool>(&tag_order).desc())
            .into_boxed();

        if let Some(org_id) = require.org_id {
            query = query.filter(hosts::org_id.eq(org_id));
        }

        if let Some(region_id) = require.scheduler.region.map(|region| region.id) {
            query = query.filter(hosts::region_id.eq(region_id));
        }

        if let Some(similarity) = require.scheduler.similarity {
            let similar = nodes::table
                .filter(nodes::host_id.eq(hosts::id))
                .filter(nodes::protocol_id.eq(require.protocol.id))
                .filter(nodes::deleted_at.is_null())
                .select(count(nodes::id))
                .single_value();

            query = match similarity {
                SimilarNodeAffinity::Cluster => query.then_order_by(similar.desc()),
                SimilarNodeAffinity::Spread => query.then_order_by(similar),
            };
        }

        query = match require.scheduler.resource {
            ResourceAffinity::MostResources => {
                query.then_order_by((free_cpu.desc(), free_memory.desc(), free_disk.desc()))
            }
            ResourceAffinity::LeastResources => {
                query.then_order_by((free_cpu, free_memory, free_disk))
            }
        };

        if let Some(limit) = limit {
            query = query.limit(limit);
        }

        query.get_results(conn).await.map_err(Error::HostCandidates)
    }

    pub fn created_by(&self) -> Resource {
        Resource::new(self.created_by_type, self.created_by_id)
    }
}

pub struct HostRequirements<'r> {
    pub scheduler: NodeScheduler,
    pub protocol: &'r Protocol,
    pub org_id: Option<OrgId>,
    pub cpu_cores: i64,
    pub memory_bytes: i64,
    pub disk_bytes: i64,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = hosts)]
pub struct NewHost<'a> {
    pub org_id: Option<OrgId>,
    pub region_id: Option<RegionId>,
    pub network_name: &'a str,
    pub display_name: Option<&'a str>,
    pub schedule_type: ScheduleType,
    pub os: &'a str,
    pub os_version: &'a str,
    pub bv_version: &'a Version,
    pub ip_address: IpNetwork,
    pub ip_gateway: IpNetwork,
    pub cpu_cores: i64,
    pub memory_bytes: i64,
    pub disk_bytes: i64,
    pub tags: Tags,
    pub created_by_type: ResourceType,
    pub created_by_id: ResourceId,
}

impl NewHost<'_> {
    pub async fn create(self, ips: &[IpNetwork], conn: &mut Conn<'_>) -> Result<Host, Error> {
        if let Some(org_id) = self.org_id {
            Org::add_host(org_id, conn).await?;
        }

        let host: Host = diesel::insert_into(hosts::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)?;

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
    pub network_name: Option<&'a str>,
    pub display_name: Option<&'a str>,
    pub region_id: Option<RegionId>,
    pub schedule_type: Option<ScheduleType>,
    pub connection_status: Option<ConnectionStatus>,
    pub os: Option<&'a str>,
    pub os_version: Option<&'a str>,
    pub bv_version: Option<&'a Version>,
    pub ip_address: Option<IpNetwork>,
    pub ip_gateway: Option<IpNetwork>,
    pub cpu_cores: Option<i64>,
    pub memory_bytes: Option<i64>,
    pub disk_bytes: Option<i64>,
    pub tags: Option<Tags>,
}

impl UpdateHost<'_> {
    pub const fn new(id: HostId) -> Self {
        UpdateHost {
            id,
            network_name: None,
            display_name: None,
            region_id: None,
            schedule_type: None,
            connection_status: None,
            os: None,
            os_version: None,
            bv_version: None,
            ip_address: None,
            ip_gateway: None,
            cpu_cores: None,
            memory_bytes: None,
            disk_bytes: None,
            tags: None,
        }
    }

    #[must_use]
    pub const fn with_connection_status(mut self, status: ConnectionStatus) -> Self {
        self.connection_status = Some(status);
        self
    }

    pub async fn update(self, conn: &mut Conn<'_>) -> Result<Host, Error> {
        if self == Self::new(self.id) {
            return Err(Error::NoUpdate);
        }

        let row = hosts::table
            .find(self.id)
            .filter(hosts::deleted_at.is_null());

        diesel::update(row)
            .set((self, hosts::updated_at.eq(Utc::now())))
            .get_result(conn)
            .await
            .map_err(Error::Update)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = hosts)]
pub struct UpdateHostMetrics {
    pub id: HostId,
    pub org_id: Option<OrgId>,
    pub used_cpu_hundreths: Option<i64>,
    pub used_memory_bytes: Option<i64>,
    pub used_disk_bytes: Option<i64>,
    pub load_one_percent: Option<f64>,
    pub load_five_percent: Option<f64>,
    pub load_fifteen_percent: Option<f64>,
    pub network_received_bytes: Option<i64>,
    pub network_sent_bytes: Option<i64>,
    pub uptime_seconds: Option<i64>,
}

impl UpdateHostMetrics {
    pub async fn update_metrics(
        updates: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Host>, Error> {
        let mut hosts = Vec::with_capacity(updates.len());

        for update in updates {
            let row = hosts::table
                .find(update.id)
                .filter(hosts::org_id.eq(update.org_id))
                .filter(hosts::deleted_at.is_null());
            let updated = diesel::update(row)
                .set(&update)
                .get_result(conn)
                .await
                .map_err(|err| Error::UpdateMetrics(err, update.id))?;
            hosts.push(updated);
        }

        Ok(hosts)
    }
}

#[derive(Debug)]
pub struct HostSearch {
    pub operator: SearchOperator,
    pub id: Option<String>,
    pub network_name: Option<String>,
    pub display_name: Option<String>,
    pub bv_version: Option<String>,
    pub os: Option<String>,
    pub ip: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum HostSort {
    NetworkName(SortOrder),
    DisplayName(SortOrder),
    Os(SortOrder),
    OsVersion(SortOrder),
    BvVersion(SortOrder),
    CpuCores(SortOrder),
    MemoryBytes(SortOrder),
    DiskBytes(SortOrder),
    NodeCount(SortOrder),
    CreatedAt(SortOrder),
    UpdatedAt(SortOrder),
}

impl HostSort {
    fn into_expr<T>(self) -> Box<dyn BoxableExpression<T, Pg, SqlType = NotSelectable>>
    where
        hosts::network_name: SelectableExpression<T>,
        hosts::display_name: SelectableExpression<T>,
        hosts::os: SelectableExpression<T>,
        hosts::os_version: SelectableExpression<T>,
        hosts::bv_version: SelectableExpression<T>,
        hosts::cpu_cores: SelectableExpression<T>,
        hosts::memory_bytes: SelectableExpression<T>,
        hosts::disk_bytes: SelectableExpression<T>,
        hosts::node_count: SelectableExpression<T>,
        hosts::created_at: SelectableExpression<T>,
        hosts::updated_at: SelectableExpression<T>,
    {
        use HostSort::*;
        use SortOrder::*;

        match self {
            NetworkName(Asc) => Box::new(hosts::network_name.asc()),
            NetworkName(Desc) => Box::new(hosts::network_name.desc()),

            DisplayName(Asc) => Box::new(hosts::display_name.asc()),
            DisplayName(Desc) => Box::new(hosts::display_name.desc()),

            Os(Asc) => Box::new(hosts::os.asc()),
            Os(Desc) => Box::new(hosts::os.desc()),

            OsVersion(Asc) => Box::new(hosts::os_version.asc()),
            OsVersion(Desc) => Box::new(hosts::os_version.desc()),

            BvVersion(Asc) => Box::new(hosts::bv_version.asc()),
            BvVersion(Desc) => Box::new(hosts::bv_version.desc()),

            CpuCores(Asc) => Box::new(hosts::cpu_cores.asc()),
            CpuCores(Desc) => Box::new(hosts::cpu_cores.desc()),

            MemoryBytes(Asc) => Box::new(hosts::memory_bytes.asc()),
            MemoryBytes(Desc) => Box::new(hosts::memory_bytes.desc()),

            DiskBytes(Asc) => Box::new(hosts::disk_bytes.asc()),
            DiskBytes(Desc) => Box::new(hosts::disk_bytes.desc()),

            NodeCount(Asc) => Box::new(hosts::node_count.asc()),
            NodeCount(Desc) => Box::new(hosts::node_count.desc()),

            CreatedAt(Asc) => Box::new(hosts::created_at.asc()),
            CreatedAt(Desc) => Box::new(hosts::created_at.desc()),

            UpdatedAt(Asc) => Box::new(hosts::updated_at.asc()),
            UpdatedAt(Desc) => Box::new(hosts::updated_at.desc()),
        }
    }
}

#[derive(Debug)]
pub struct HostFilter {
    pub org_ids: Vec<OrgId>,
    pub versions: Vec<String>,
    pub offset: u64,
    pub limit: u64,
    pub search: Option<HostSearch>,
    pub sort: VecDeque<HostSort>,
}

impl HostFilter {
    pub async fn query(mut self, conn: &mut Conn<'_>) -> Result<(Vec<Host>, u64), Error> {
        let mut query = hosts::table
            .filter(hosts::deleted_at.is_null())
            .into_boxed();

        if let Some(search) = self.search {
            query = query.filter(search.into_expression());
        }

        if !self.org_ids.is_empty() {
            query = query.filter(hosts::org_id.eq_any(self.org_ids));
        }

        if !self.versions.is_empty() {
            query = query.filter(hosts::bv_version.eq_any(self.versions));
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
                    predicate = Box::new(predicate.or(sql::text(hosts::id).like(id)));
                }
                if let Some(name) = self.network_name {
                    predicate = Box::new(predicate.or(sql::lower(hosts::network_name).like(name)));
                }
                if let Some(version) = self.bv_version {
                    predicate = Box::new(predicate.or(sql::lower(hosts::bv_version).like(version)));
                }
                if let Some(os) = self.os {
                    predicate = Box::new(predicate.or(sql::lower(hosts::os).like(os)));
                }
                if let Some(ip) = self.ip {
                    predicate = Box::new(predicate.or(sql::text(hosts::ip_address).like(ip)));
                }
                predicate
            }
            SearchOperator::And => {
                let mut predicate: Box<dyn BoxableExpression<hosts::table, Pg, SqlType = Bool>> =
                    Box::new(true.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.and(sql::text(hosts::id).like(id)));
                }
                if let Some(name) = self.network_name {
                    predicate = Box::new(predicate.and(sql::lower(hosts::network_name).like(name)));
                }
                if let Some(version) = self.bv_version {
                    predicate =
                        Box::new(predicate.and(sql::lower(hosts::bv_version).like(version)));
                }
                if let Some(os) = self.os {
                    predicate = Box::new(predicate.and(sql::lower(hosts::os).like(os)));
                }
                if let Some(ip) = self.ip {
                    predicate = Box::new(predicate.and(sql::text(hosts::ip_address).like(ip)));
                }
                predicate
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumScheduleType"]
pub enum ScheduleType {
    Automatic,
    Manual,
}

impl From<ScheduleType> for api::ScheduleType {
    fn from(schedule_type: ScheduleType) -> Self {
        match schedule_type {
            ScheduleType::Automatic => api::ScheduleType::Automatic,
            ScheduleType::Manual => api::ScheduleType::Manual,
        }
    }
}

impl TryFrom<api::ScheduleType> for ScheduleType {
    type Error = Error;

    fn try_from(schedule_type: api::ScheduleType) -> Result<Self, Self::Error> {
        match schedule_type {
            api::ScheduleType::Unspecified => Err(Error::UnknownScheduleType),
            api::ScheduleType::Automatic => Ok(ScheduleType::Automatic),
            api::ScheduleType::Manual => Ok(ScheduleType::Manual),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumConnectionStatus"]
pub enum ConnectionStatus {
    Online,
    Offline,
}

impl TryFrom<api::HostConnectionStatus> for ConnectionStatus {
    type Error = Error;

    fn try_from(status: api::HostConnectionStatus) -> Result<Self, Self::Error> {
        match status {
            api::HostConnectionStatus::Unspecified => Err(Error::UnknownConnectionStatus),
            api::HostConnectionStatus::Offline => Ok(ConnectionStatus::Offline),
            api::HostConnectionStatus::Online => Ok(ConnectionStatus::Online),
        }
    }
}
