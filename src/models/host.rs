use std::collections::HashMap;

use chrono::{DateTime, Utc};
use diesel::{dsl, prelude::*};
use diesel_async::RunQueryDsl;

use crate::auth::resource::{HostId, OrgId, UserId};
use crate::{cookbook::script::HardwareRequirements, Result};

use super::schema::hosts;
use super::Paginate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumConnStatus"]
pub enum ConnectionStatus {
    Online,
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumHostType"]
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
    pub ip_range_from: ipnetwork::IpNetwork,
    pub ip_range_to: ipnetwork::IpNetwork,
    pub ip_gateway: ipnetwork::IpNetwork,
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
    pub region_id: Option<uuid::Uuid>,
}

#[derive(Clone, Debug)]
pub struct HostFilter {
    pub org_id: OrgId,
    pub offset: u64,
    pub limit: u64,
}

pub struct HostRequirements {
    pub requirements: HardwareRequirements,
    pub blockchain_id: uuid::Uuid,
    pub node_type: super::NodeType,
    pub host_type: Option<super::HostType>,
    pub scheduler: super::NodeScheduler,
    pub org_id: Option<OrgId>,
}

impl Host {
    pub async fn find_by_id(id: HostId, conn: &mut super::Conn) -> Result<Self> {
        let host = hosts::table.find(id).get_result(conn).await?;
        Ok(host)
    }

    pub async fn find_by_ids(mut ids: Vec<HostId>, conn: &mut super::Conn) -> Result<Vec<Self>> {
        ids.sort();
        ids.dedup();
        let hosts = hosts::table
            .filter(hosts::id.eq_any(ids))
            .get_results(conn)
            .await?;
        Ok(hosts)
    }

    pub async fn by_ids(ids: &[HostId], conn: &mut super::Conn) -> Result<Vec<Self>> {
        let hosts: Vec<Self> = hosts::table
            .filter(hosts::id.eq_any(ids))
            .get_results(conn)
            .await?;
        let hosts_map: HashMap<_, _> = hosts.into_iter().map(|h| (h.id, h)).collect();
        Ok(ids.iter().map(|id| hosts_map[id].clone()).collect())
    }

    /// For each provided argument, filters the hosts by that argument.
    pub async fn filter(filter: HostFilter, conn: &mut super::Conn) -> Result<(u64, Vec<Self>)> {
        let HostFilter {
            org_id,
            offset,
            limit,
        } = filter;
        let query = hosts::table.filter(hosts::org_id.eq(org_id)).into_boxed();

        let (total, hosts) = query
            .paginate(limit.try_into()?, offset.try_into()?)
            .get_results_counted(conn)
            .await?;
        Ok((total.try_into()?, hosts))
    }

    pub async fn find_by_name(name: &str, conn: &mut super::Conn) -> Result<Self> {
        let host = hosts::table
            .filter(hosts::name.eq(name))
            .get_result(conn)
            .await?;
        Ok(host)
    }

    pub async fn delete(id: HostId, conn: &mut super::Conn) -> Result<usize> {
        let n_rows = diesel::delete(hosts::table.find(id)).execute(conn).await?;
        Ok(n_rows)
    }

    /// This function returns a list of up to 2 possible hosts that the node may be scheduled on.
    /// This list is ordered by suitability, the best fit will be first in the list. Note that zero
    /// hosts may be returned when our system is out of resources, and this case should be handled
    /// gracefully.
    pub async fn host_candidates(
        reqs: HostRequirements,
        limit: Option<i64>,
        conn: &mut super::Conn,
    ) -> crate::Result<Vec<Host>> {
        use super::schema::sql_types::EnumNodeType;
        use diesel::sql_types::{BigInt, Nullable, Uuid};

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
                hosts.cpu_count - (SELECT COALESCE(SUM(vcpu_count), 0)::BIGINT FROM nodes WHERE host_id = hosts.id) AS av_cpus,
                hosts.mem_size_bytes - (SELECT COALESCE(SUM(mem_size_bytes), 0)::BIGINT FROM nodes WHERE host_id = hosts.id) AS av_mem,
                hosts.disk_size_bytes - (SELECT COALESCE(SUM(disk_size_bytes), 0)::BIGINT FROM nodes WHERE host_id = hosts.id) AS av_disk,
                (SELECT COUNT(*) FROM ip_addresses WHERE ip_addresses.host_id = hosts.id AND NOT ip_addresses.is_assigned) AS ips,
                (SELECT COUNT(*) FROM nodes WHERE host_id = hosts.id AND blockchain_id = $4 AND node_type = $5 AND host_type = 'cloud') AS n_similar,
                hosts.region_id AS region_id,
                hosts.org_id AS org_id,
                hosts.host_type AS host_type
            FROM
                hosts
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

        let hosts: Vec<HostCandidate> = diesel::sql_query(query)
            .bind::<BigInt, _>(requirements.vcpu_count as i64)
            .bind::<BigInt, _>(requirements.mem_size_mb as i64 * 1000 * 1000)
            .bind::<BigInt, _>(requirements.disk_size_gb as i64 * 1000 * 1000 * 1000)
            .bind::<Uuid, _>(blockchain_id)
            .bind::<EnumNodeType, _>(node_type)
            .bind::<Nullable<BigInt>, _>(limit)
            .bind::<Nullable<Uuid>, _>(scheduler.region.map(|r| r.id))
            .bind::<Nullable<Uuid>, _>(org_id)
            .bind::<Nullable<crate::models::schema::sql_types::EnumHostType>, _>(host_type)
            .get_results(conn)
            .await?;
        let host_ids: Vec<_> = hosts.into_iter().map(|h| h.host_id).collect();

        Self::by_ids(&host_ids, conn).await
    }

    pub async fn node_counts(
        hosts: &[Self],
        conn: &mut super::Conn,
    ) -> crate::Result<HashMap<uuid::Uuid, u64>> {
        use super::schema::nodes;

        let mut host_ids: Vec<_> = hosts.iter().map(|h| h.id).collect();
        host_ids.sort();
        host_ids.dedup();

        let counts: Vec<(uuid::Uuid, i64)> = nodes::table
            .filter(nodes::host_id.eq_any(host_ids))
            .group_by(nodes::host_id)
            .select((nodes::host_id, dsl::count(nodes::id)))
            .get_results(conn)
            .await?;
        counts
            .into_iter()
            .map(|(host, count)| Ok((host, count.try_into()?)))
            .collect()
    }

    /// Returns the possible list of regions for the
    pub async fn regions_for(
        org_id: OrgId,
        blockchain: super::Blockchain,
        node_type: super::NodeType,
        requirements: HardwareRequirements,
        host_type: Option<HostType>,
        conn: &mut super::Conn,
    ) -> crate::Result<Vec<super::Region>> {
        let scheduler = super::NodeScheduler {
            region: None,
            similarity: None,
            resource: super::ResourceAffinity::LeastResources,
        };
        let requirements = super::HostRequirements {
            requirements,
            blockchain_id: blockchain.id,
            node_type,
            host_type,
            scheduler,
            org_id: Some(org_id),
        };
        let regions = Self::host_candidates(requirements, None, conn)
            .await?
            .into_iter()
            .flat_map(|host| host.region_id)
            .collect();
        super::Region::by_ids(regions, conn).await
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
    pub ip_range_from: ipnetwork::IpNetwork,
    pub ip_range_to: ipnetwork::IpNetwork,
    pub ip_gateway: ipnetwork::IpNetwork,
    /// The id of the org that owns and operates this host.
    pub org_id: OrgId,
    /// This is the id of the user that created this host.
    pub created_by: UserId,
    // The id of the region where this host is located.
    pub region_id: Option<uuid::Uuid>,
}

impl NewHost<'_> {
    /// Creates a new `Host` in the db, including the necessary related rows.
    pub async fn create(self, conn: &mut super::Conn) -> Result<Host> {
        let ip_addr = self.ip_addr.parse()?;
        let ip_gateway = self.ip_gateway.ip();
        let ip_range_from = self.ip_range_from.ip();
        let ip_range_to = self.ip_range_to.ip();

        let host: Host = diesel::insert_into(hosts::table)
            .values(self)
            .get_result(conn)
            .await?;

        // Create IP range for new host
        let create_range = super::NewIpAddressRange::try_new(ip_range_from, ip_range_to, host.id)?;
        create_range.create(&[ip_addr, ip_gateway], conn).await?;

        Ok(host)
    }
}

#[derive(Debug, Clone, AsChangeset)]
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
    pub ip_range_from: Option<ipnetwork::IpNetwork>,
    pub ip_range_to: Option<ipnetwork::IpNetwork>,
    pub ip_gateway: Option<ipnetwork::IpNetwork>,
}

impl UpdateHost<'_> {
    pub async fn update(self, conn: &mut super::Conn) -> Result<Host> {
        let host = diesel::update(hosts::table.find(self.id))
            .set(self)
            .get_result(conn)
            .await?;
        Ok(host)
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
    pub async fn update_metrics(updates: Vec<Self>, conn: &mut super::Conn) -> Result<Vec<Host>> {
        let mut results = Vec::with_capacity(updates.len());
        for update in updates {
            let updated = diesel::update(hosts::table.find(update.id))
                .set(update)
                .get_result(conn)
                .await?;
            results.push(updated);
        }
        Ok(results)
    }
}

#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = hosts)]
pub struct HostStatusRequest {
    pub version: Option<String>,
    pub status: ConnectionStatus,
}
