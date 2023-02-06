use super::{NodeProvision, PgQuery};
use crate::auth::{FindableById, HostAuthToken, Identifiable, JwtToken, Owned, TokenError};
use crate::cookbook::HardwareRequirements;
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy::{self, HostInfo};
use crate::grpc::helpers::required;
use crate::models::{IpAddress, IpAddressRangeRequest, UpdateInfo};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{FromRow, Row};
use std::convert::From;
use std::net::IpAddr;
use uuid::Uuid;

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_conn_status", rename_all = "snake_case")]
pub enum ConnectionStatus {
    Online,
    #[default]
    Offline,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_conn_status", rename_all = "snake_case")]
pub enum HostType {
    #[default]
    Cloud,
    Enterprise,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: Uuid,
    pub name: String,
    pub version: Option<String>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub location: Option<String>,
    pub ip_addr: String,
    pub status: ConnectionStatus, //TODO: change to is_online:bool
    pub created_at: DateTime<Utc>,
    pub ip_range_from: Option<IpAddr>,
    pub ip_range_to: Option<IpAddr>,
    pub ip_gateway: Option<IpAddr>,

    /* metrics */
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

impl TryFrom<PgRow> for Host {
    type Error = sqlx::Error;

    fn try_from(row: PgRow) -> Result<Self, sqlx::Error> {
        let host = Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            cpu_count: row.try_get("cpu_count")?,
            mem_size: row.try_get("mem_size")?,
            disk_size: row.try_get("disk_size")?,
            os: row.try_get("os")?,
            os_version: row.try_get("os_version")?,
            version: row.try_get("version")?,
            location: row.try_get("location")?,
            ip_addr: row.try_get("ip_addr")?,
            status: row.try_get("status")?,
            created_at: row.try_get("created_at")?,
            ip_range_from: row.try_get("ip_range_from")?,
            ip_range_to: row.try_get("ip_range_to")?,
            ip_gateway: row.try_get("ip_gateway")?,
            used_cpu: row.try_get("used_cpu")?,
            used_memory: row.try_get("used_memory")?,
            used_disk_space: row.try_get("used_disk_space")?,
            load_one: row.try_get("load_one")?,
            load_five: row.try_get("load_five")?,
            load_fifteen: row.try_get("load_fifteen")?,
            network_received: row.try_get("network_received")?,
            network_sent: row.try_get("network_sent")?,
            uptime: row.try_get("uptime")?,
        };

        Ok(host)
    }
}

impl Host {
    /// Test if given `token` has expired and refresh it using the `refresh_token` if necessary
    pub fn verify_auth_token(token: HostAuthToken) -> Result<HostAuthToken> {
        if token.has_expired() {
            Err(ApiError::from(TokenError::Expired))
        } else {
            // Token is valid, just return what we got
            // If nothing was updated or changed, we don't even query for the user to save 1 query
            Ok(token)
        }
    }

    pub async fn toggle_online(id: Uuid, is_online: bool, tx: &mut super::DbTrx<'_>) -> Result<()> {
        let status = if is_online {
            ConnectionStatus::Online
        } else {
            ConnectionStatus::Offline
        };

        sqlx::query("UPDATE hosts SET status = $1 WHERE id = $2 RETURNING *;")
            .bind(status)
            .bind(id)
            .fetch_one(tx)
            .await?;

        Ok(())
    }

    pub async fn find_all(db: &mut sqlx::PgConnection) -> Result<Vec<Self>> {
        sqlx::query("SELECT * FROM hosts order by lower(name)")
            .try_map(Self::try_from)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_node(node_id: Uuid, db: &mut sqlx::PgConnection) -> Result<Self> {
        sqlx::query("SELECT hosts.* FROM nodes INNER JOIN hosts ON nodes.host_id = hosts.id WHERE nodes.id = $1")
            .bind(node_id)
            .try_map(Self::try_from)
            .fetch_one(db)
            .await
            .map_err(Into::into)
    }

    pub async fn find_by_id(host_id: Uuid, db: &mut sqlx::PgConnection) -> Result<Self> {
        sqlx::query("SELECT * FROM hosts WHERE id = $1")
            .bind(host_id)
            .try_map(Self::try_from)
            .fetch_one(db)
            .await
            .map_err(Into::into)
    }

    /// Creates a new `Host` in the db, including the necessary related rows.
    pub async fn create(req: HostRequest, tx: &mut super::DbTrx<'_>) -> Result<Self> {
        // Ensure gateway IP is not amongst the ones created in the IP range
        if IpAddress::in_range(
            req.ip_gateway
                .ok_or_else(required("IP needed for in_range test"))?,
            req.ip_range_from
                .ok_or_else(required("IP range FROM needed for in_range test"))?,
            req.ip_range_to
                .ok_or_else(required("IP range TO for in_range test"))?,
        ) {
            return Err(ApiError::IpGatewayError(anyhow!(
                "{:?} is in range {:?} - {:?}",
                req.ip_gateway,
                req.ip_range_from,
                req.ip_range_to
            )));
        }

        let host = sqlx::query(
            r#"INSERT INTO hosts 
            (
                name,
                version,
                location,
                ip_addr,
                status,
                cpu_count,
                mem_size,
                disk_size,
                os,
                os_version,
                ip_gateway,
                ip_range_from,
                ip_range_to
            ) 
            VALUES 
            ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING *"#,
        )
        .bind(req.name)
        .bind(req.version)
        .bind(req.location)
        .bind(req.ip_addr)
        .bind(req.status)
        .bind(req.cpu_count)
        .bind(req.mem_size)
        .bind(req.disk_size)
        .bind(req.os)
        .bind(req.os_version)
        .bind(req.ip_gateway)
        .bind(req.ip_range_from)
        .bind(req.ip_range_to)
        .try_map(Self::try_from)
        .fetch_one(&mut *tx)
        .await?;

        // Create IP range for new host
        let req = IpAddressRangeRequest::try_new(
            host.ip_range_from.ok_or_else(required("ip.range.from"))?,
            host.ip_range_to.ok_or_else(required("ip.range.to"))?,
            Some(host.id),
        )?;
        IpAddress::create_range(req, tx).await?;

        Ok(host)
    }

    #[deprecated(since = "0.2.0", note = "deprecated in favor of 'update_all'")]
    pub async fn update(id: Uuid, host: HostRequest, tx: &mut super::DbTrx<'_>) -> Result<Self> {
        let host = sqlx::query(
            r#"UPDATE hosts SET name = $1, version = $2, location = $3, ip_addr = $4, status = $6, cpu_count = $7, mem_size = $8, disk_size = $9, os = $10, os_version = $11 WHERE id = $12 RETURNING *"#
        )
        .bind(host.name)
        .bind(host.version)
        .bind(host.location)
        .bind(host.ip_addr)
        .bind(host.status)
        .bind(host.cpu_count)
        .bind(host.mem_size)
        .bind(host.disk_size)
        .bind(host.os)
        .bind(host.os_version)
        .bind(id)
        .try_map(Self::try_from)
        .fetch_one(tx)
        .await?;
        Ok(host)
    }

    pub async fn update_all(
        fields: HostSelectiveUpdate,
        tx: &mut super::DbTrx<'_>,
    ) -> Result<Self> {
        let host = sqlx::query(
            r#"UPDATE hosts SET 
                    name = COALESCE($1, name),
                    version = COALESCE($2, version),
                    location = COALESCE($3, location),
                    cpu_count = COALESCE($4, cpu_count),
                    mem_size = COALESCE($5, mem_size),
                    disk_size = COALESCE($6, disk_size),
                    os = COALESCE($7, os),
                    os_version = COALESCE($8, os_version),
                    ip_addr = COALESCE($9, ip_addr),
                    status = COALESCE($10, status)
                WHERE id = $11 RETURNING *"#,
        )
        .bind(fields.name)
        .bind(fields.version)
        .bind(fields.location)
        .bind(fields.cpu_count)
        .bind(fields.mem_size)
        .bind(fields.disk_size)
        .bind(fields.os)
        .bind(fields.os_version)
        .bind(fields.ip_addr)
        .bind(fields.status)
        .bind(fields.id)
        .try_map(Self::try_from)
        .fetch_one(tx)
        .await?;
        Ok(host)
    }

    pub async fn update_status(
        id: Uuid,
        host: HostStatusRequest,
        tx: &mut super::DbTrx<'_>,
    ) -> Result<Self> {
        let host =
            sqlx::query(r#"UPDATE hosts SET version = $1, status = $2  WHERE id = $3 RETURNING *"#)
                .bind(host.version)
                .bind(host.status)
                .bind(id)
                .try_map(Self::try_from)
                .fetch_one(tx)
                .await?;
        Ok(host)
    }

    pub async fn delete(id: Uuid, tx: &mut super::DbTrx<'_>) -> Result<u64> {
        let deleted = sqlx::query("DELETE FROM hosts WHERE id = $1")
            .bind(id)
            .execute(tx)
            .await?;
        Ok(deleted.rows_affected())
    }

    /// We sum up all nodes values assigned to a host and deduct that from the total the host has
    /// We don't consider CPUs in the selection, hard disk is more important than memory. The result
    /// is ordered by disk_size and mem_size the first one in the list is returned
    pub async fn get_next_available_host_id(
        requirements: HardwareRequirements,
        db: &mut sqlx::PgConnection,
    ) -> Result<Uuid> {
        let host = sqlx::query(
            r#"
            SELECT hosts.id as h_id,
                   COALESCE((hosts.mem_size - (SELECT SUM(mem_size_mb) from nodes where host_id = hosts.id)::BIGINT), 0) as mem_size,
                   COALESCE((hosts.disk_size - (SELECT SUM(disk_size_gb) from nodes where host_id = hosts.id)::BIGINT), 0) as disk_size,
                   (select count(*) from ip_addresses where ip_addresses.host_id = hosts.id and not ip_addresses.is_assigned)::BIGINT as ip_addrs
            FROM hosts
            ORDER BY ip_addrs desc, disk_size desc, mem_size desc
            LIMIT 1
        "#,
        )
        .fetch_one(db)
        .await?;
        let host_id = host.try_get::<Uuid, _>("h_id")?;
        let disk_size: i64 = host.try_get::<i64, _>("disk_size")?;
        let mem_size: i64 = host.try_get::<i64, _>("mem_size")?;
        let ip_addrs: i64 = host.try_get::<i64, _>("ip_addrs")?;

        // Trace warnings, if the selected host doesn't seem to have enough resources
        if *requirements.disk_size_gb() > disk_size / 1024i64.pow(3) {
            tracing::warn!(
                "Host {} doesn't seem to have enough disk space available",
                host_id
            );
        }
        if *requirements.mem_size_mb() > mem_size / 1024i64.pow(2) {
            tracing::warn!(
                "Host {} doesn't seem to have enough memory available",
                host_id
            );
        }
        if ip_addrs < 1 {
            tracing::warn!(
                "Host {} doesn't seem to have enough IP addresses available",
                host_id
            );
        }

        Ok(host_id)
    }
}

#[axum::async_trait]
impl FindableById for Host {
    async fn find_by_id(id: Uuid, db: &mut sqlx::PgConnection) -> Result<Self> {
        sqlx::query("SELECT * FROM hosts WHERE id = $1")
            .bind(id)
            .try_map(Self::try_from)
            .fetch_one(db)
            .await
            .map_err(Into::into)
    }
}

impl Identifiable for Host {
    fn get_id(&self) -> Uuid {
        self.id
    }
}

#[axum::async_trait]
impl Owned<Host, ()> for Host {
    async fn is_owned_by(&self, resource: Host, _db: ()) -> bool {
        self.id == resource.id
    }
}

#[tonic::async_trait]
impl UpdateInfo<HostInfo, Host> for Host {
    async fn update_info(info: HostInfo, tx: &mut super::DbTrx<'_>) -> Result<Host> {
        let id: Uuid = info.id.ok_or_else(required("info.id"))?.parse()?;
        let host = sqlx::query(
            r##"UPDATE hosts SET
                         name = COALESCE($1, name),
                         version = COALESCE($2, version),
                         location = COALESCE($3, location),
                         cpu_count = COALESCE($4, cpu_count),
                         mem_size = COALESCE($5, mem_size),
                         disk_size = COALESCE($6, disk_size),
                         os = COALESCE($7, os),
                         os_version = COALESCE($8, os_version),
                         ip_addr = COALESCE($9, ip_addr),
                WHERE id = $10
                RETURNING *
            "##,
        )
        .bind(info.name)
        .bind(info.version)
        .bind(info.location)
        .bind(info.cpu_count)
        .bind(info.mem_size)
        .bind(info.disk_size)
        .bind(info.os)
        .bind(info.os_version)
        .bind(info.ip)
        .bind(id)
        .fetch_one(tx)
        .await?;

        Ok(host.try_into()?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostRequest {
    pub name: String,
    pub version: Option<String>,
    pub location: Option<String>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub ip_addr: String,
    pub status: ConnectionStatus,
    pub ip_range_from: Option<IpAddr>,
    pub ip_range_to: Option<IpAddr>,
    pub ip_gateway: Option<IpAddr>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HostSelectiveUpdate {
    pub id: Uuid,
    pub name: Option<String>,
    pub version: Option<String>,
    pub location: Option<String>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub ip_addr: Option<String>,
    pub status: Option<ConnectionStatus>,
    pub ip_range_from: Option<IpAddr>,
    pub ip_range_to: Option<IpAddr>,
    pub ip_gateway: Option<IpAddr>,

    // -- These fields are related to the metrics of the host --
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

impl HostSelectiveUpdate {
    /// Performs a selective update of only the columns related to metrics of the provided nodes.
    pub async fn update_metrics(updates: Vec<Self>, tx: &mut super::DbTrx<'_>) -> Result<()> {
        type PgBuilder = sqlx::QueryBuilder<'static, sqlx::Postgres>;

        // Lets not perform a malformed query on empty input, but lets instead be fast and
        // short-circuit here.
        if updates.is_empty() {
            return Ok(());
        }

        // We first start the query out by declaring which fields to update.
        let mut query_builder = PgBuilder::new(
            "UPDATE hosts SET
                used_cpu = row.used_cpu,
                used_memory = row.used_memory,
                used_disk_space = row.used_disk_space,
                load_one = row.load_one,
                load_five = row.load_five,
                load_fifteen = row.load_fifteen,
                network_received = row.network_received,
                network_sent = row.network_sent,
                uptime = row.uptime
            FROM (
                ",
        );

        // Now we bind a variable number of parameters
        query_builder.push_values(updates.iter(), |mut builder, update| {
            builder
                .push_bind(update.id)
                .push_bind(update.used_cpu)
                .push_bind(update.used_memory)
                .push_bind(update.used_disk_space)
                .push_bind(update.load_one)
                .push_bind(update.load_five)
                .push_bind(update.load_fifteen)
                .push_bind(update.network_received)
                .push_bind(update.network_sent)
                .push_bind(update.uptime);
        });
        // We finish the query by specifying which bind parameters mean what. NOTE: When adding
        // bind parameters they MUST be bound in the same order as they are specified below. Not
        // doing so results in incorrectly interpreted queries.
        query_builder.push(
            "
            ) AS row(
                id, used_cpu, used_memory, used_disk_space, load_one, load_five, load_fifteen,
                network_received, network_sent, uptime
            ) WHERE
                hosts.id = row.id::uuid;",
        );
        let template = sqlx::query(query_builder.sql());
        let query = updates.into_iter().fold(template, Self::bind_to);
        query.execute(tx).await?;
        Ok(())
    }

    pub fn from_metrics(id: String, metric: blockjoy::HostMetrics) -> Result<Self> {
        let id = id.parse()?;
        Ok(Self {
            id,
            used_cpu: metric.used_cpu.map(i32::try_from).transpose()?,
            used_memory: metric.used_memory.map(i64::try_from).transpose()?,
            used_disk_space: metric.used_disk_space.map(i64::try_from).transpose()?,
            load_one: metric.load_one,
            load_five: metric.load_five,
            load_fifteen: metric.load_fifteen,
            network_received: metric.network_received.map(i64::try_from).transpose()?,
            network_sent: metric.network_sent.map(i64::try_from).transpose()?,
            uptime: metric.uptime.map(i64::try_from).transpose()?,
            ..Default::default()
        })
    }

    /// Binds the params in `params` to the provided query in the correct order, then returns the
    /// modified query. Since this is order-dependent, this function is private.
    fn bind_to(query: PgQuery<'_>, params: Self) -> PgQuery<'_> {
        query
            .bind(params.id)
            .bind(params.used_cpu)
            .bind(params.used_memory)
            .bind(params.used_disk_space)
            .bind(params.load_one)
            .bind(params.load_five)
            .bind(params.load_fifteen)
            .bind(params.network_received)
            .bind(params.network_sent)
            .bind(params.uptime)
    }
}

impl From<HostCreateRequest> for HostRequest {
    fn from(host: HostCreateRequest) -> Self {
        Self {
            name: host.name,
            version: host.version,
            location: host.location,
            cpu_count: host.cpu_count,
            mem_size: host.mem_size,
            disk_size: host.disk_size,
            os: host.os,
            os_version: host.os_version,
            ip_addr: host.ip_addr,
            status: ConnectionStatus::Offline,
            ip_range_from: host.ip_range_from,
            ip_range_to: host.ip_range_to,
            ip_gateway: host.ip_gateway,
        }
    }
}

impl TryFrom<HostInfo> for HostSelectiveUpdate {
    type Error = ApiError;

    fn try_from(info: HostInfo) -> Result<Self, Self::Error> {
        let update = Self {
            id: info.id.ok_or_else(required("info.id"))?.parse()?,
            name: info.name,
            version: info.version,
            location: info.location,
            cpu_count: info.cpu_count,
            mem_size: info.mem_size,
            disk_size: info.disk_size,
            os: info.os,
            os_version: info.os_version,
            ip_addr: info.ip,
            status: None,
            ip_range_from: info.ip_range_from.map(|ip| ip.parse()).transpose()?,
            ip_range_to: info.ip_range_to.map(|ip| ip.parse()).transpose()?,
            ip_gateway: info.ip_gateway.map(|ip| ip.parse()).transpose()?,
            ..Default::default()
        };
        Ok(update)
    }
}

impl TryFrom<blockjoy::ProvisionHostRequest> for HostCreateRequest {
    type Error = ApiError;

    fn try_from(request: blockjoy::ProvisionHostRequest) -> Result<Self> {
        let host_info = request.info.ok_or_else(required("info"))?;
        let ip_range_from = if host_info.ip_range_from.is_some() {
            Some(
                host_info
                    .ip_range_from
                    .unwrap()
                    .parse::<IpAddr>()
                    .map_err(|e| {
                        ApiError::UnexpectedError(anyhow!(
                            "IP range FROM required in HostCreateRequest::try_from: {}",
                            e
                        ))
                    })?,
            )
        } else {
            None
        };
        let ip_range_to = if host_info.ip_range_to.is_some() {
            Some(
                host_info
                    .ip_range_to
                    .unwrap()
                    .parse::<IpAddr>()
                    .map_err(|e| {
                        ApiError::UnexpectedError(anyhow!(
                            "IP range TO required in HostCreateRequest::try_from: {}",
                            e
                        ))
                    })?,
            )
        } else {
            None
        };
        let ip_gateway = if host_info.ip_gateway.is_some() {
            Some(
                host_info
                    .ip_gateway
                    .unwrap()
                    .parse::<IpAddr>()
                    .map_err(|e| {
                        ApiError::UnexpectedError(anyhow!(
                            "IP GATEWAY required in HostCreateRequest::try_from: {}",
                            e
                        ))
                    })?,
            )
        } else {
            None
        };
        let req = Self {
            name: host_info.name.ok_or_else(required("info.name"))?,
            version: host_info.version,
            location: host_info.location,
            cpu_count: host_info.cpu_count,
            mem_size: host_info.mem_size,
            disk_size: host_info.disk_size,
            os: host_info.os,
            os_version: host_info.os_version,
            ip_addr: host_info.ip.ok_or_else(required("info.ip"))?,
            ip_range_from,
            ip_range_to,
            ip_gateway,
        };
        Ok(req)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostCreateRequest {
    pub name: String,
    pub version: Option<String>,
    pub location: Option<String>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub ip_addr: String,
    pub ip_range_from: Option<IpAddr>,
    pub ip_range_to: Option<IpAddr>,
    pub ip_gateway: Option<IpAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostStatusRequest {
    pub version: Option<String>,
    pub status: ConnectionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct HostProvision {
    pub id: String,
    pub nodes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub install_cmd: Option<String>,
    pub host_id: Option<Uuid>,
    pub ip_range_from: Option<IpAddr>,
    pub ip_range_to: Option<IpAddr>,
    pub ip_gateway: Option<IpAddr>,
}

impl HostProvision {
    pub async fn create(
        req: HostProvisionRequest,
        tx: &mut super::DbTrx<'_>,
    ) -> Result<HostProvision> {
        let nodes_str = serde_json::to_string(&req.nodes)
            .map_err(|_| ApiError::from(anyhow::anyhow!("Couldn't parse nodes")))?;

        let mut host_provision = sqlx::query_as::<_, HostProvision>(
            r#"INSERT INTO host_provisions (id, nodes, ip_range_from, ip_range_to, ip_gateway)
                   values ($1, $2, $3, $4, $5) RETURNING *"#,
        )
        .bind(Self::generate_token())
        .bind(nodes_str)
        .bind(req.ip_range_from)
        .bind(req.ip_range_to)
        .bind(req.ip_gateway)
        .fetch_one(tx)
        .await?;

        host_provision.set_install_cmd();

        Ok(host_provision)
    }

    pub async fn find_by_id<'a, Db: sqlx::PgExecutor<'a>>(
        host_provision_id: &str,
        db: Db,
    ) -> Result<HostProvision> {
        let mut host_provision =
            sqlx::query_as::<_, HostProvision>("SELECT * FROM host_provisions where id = $1")
                .bind(host_provision_id)
                .fetch_one(db)
                .await?;
        host_provision.set_install_cmd();

        Ok(host_provision)
    }

    /// Wrapper for HostProvision::claim, taking ProvisionHostRequest received via gRPC instead of HostCreateRequest
    pub async fn claim_by_grpc_provision(
        otp: &str,
        request: blockjoy::ProvisionHostRequest,
        tx: &mut super::DbTrx<'_>,
    ) -> Result<Host> {
        let request = HostCreateRequest::try_from(request)?;

        let prov = HostProvision::claim(otp, request, tx).await?;
        Ok(prov)
    }

    pub async fn claim(
        host_provision_id: &str,
        mut req: HostCreateRequest,
        tx: &mut super::DbTrx<'_>,
    ) -> Result<Host> {
        let host_provision = Self::find_by_id(host_provision_id, &mut *tx).await?;

        if host_provision.is_claimed() {
            return Err(anyhow::anyhow!("Host provision has already been claimed.").into());
        }

        req.ip_range_from = Some(
            host_provision
                .ip_range_from
                .ok_or_else(|| anyhow!("No FROM in ip range"))?,
        );
        req.ip_range_to = Some(
            host_provision
                .ip_range_to
                .ok_or_else(|| anyhow!("No TO in ip range"))?,
        );
        req.ip_gateway = Some(
            host_provision
                .ip_gateway
                .ok_or_else(|| anyhow!("No IP gateway"))?,
        );

        let host = Host::create(req.into(), tx).await?;

        sqlx::query("UPDATE host_provisions set claimed_at = now(), host_id = $1 where id = $2")
            .bind(host.id)
            .bind(host_provision.id)
            .execute(tx)
            .await?;

        Ok(host)
    }

    /// Used to populate the install_cmd field
    fn set_install_cmd(&mut self) {
        self.install_cmd = Some(format!("curl http://bvs.sh | bash -s -- {}", self.id));
    }

    pub fn is_claimed(&self) -> bool {
        self.claimed_at.is_some()
    }

    fn generate_token() -> String {
        random_string::generate(
            8,
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890",
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostProvisionRequest {
    pub nodes: Option<Vec<NodeProvision>>,
    pub ip_range_from: IpAddr,
    pub ip_range_to: IpAddr,
    pub ip_gateway: IpAddr,
}
