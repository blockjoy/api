use super::{validator::Validator, Node, NodeProvision, PgQuery};
use crate::auth::{FindableById, HostAuthToken, Identifiable, JwtToken, Owned, TokenError};
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy::{self, HostInfo};
use crate::grpc::helpers::required;
use crate::models::validator::ValidatorRequest;
use crate::models::{IpAddress, IpAddressRangeRequest, UpdateInfo};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{FromRow, PgPool, Row};
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

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub name: String,
    pub version: Option<String>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub location: Option<String>,
    pub ip_addr: String,
    pub val_ip_addrs: Option<String>,
    pub status: ConnectionStatus, //TODO: change to is_online:bool
    pub validators: Option<Vec<Validator>>,
    pub nodes: Option<Vec<Node>>,
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
            org_id: row.try_get("org_id")?,
            name: row.try_get("name")?,
            cpu_count: row.try_get("cpu_count")?,
            mem_size: row.try_get("mem_size")?,
            disk_size: row.try_get("disk_size")?,
            os: row.try_get("os")?,
            os_version: row.try_get("os_version")?,
            version: row.try_get("version")?,
            location: row.try_get("location")?,
            ip_addr: row.try_get("ip_addr")?,
            val_ip_addrs: row.try_get("val_ip_addrs")?,
            status: row.try_get("status")?,
            validators: None,
            nodes: None,
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

    pub async fn toggle_online(id: Uuid, is_online: bool, db: &PgPool) -> Result<()> {
        let status = if is_online {
            ConnectionStatus::Online
        } else {
            ConnectionStatus::Offline
        };

        sqlx::query("UPDATE hosts SET status = $1 WHERE id = $2 RETURNING *;")
            .bind(status)
            .bind(id)
            .fetch_one(db)
            .await?;

        Ok(())
    }

    pub async fn find_all(db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query("SELECT * FROM hosts order by lower(name)")
            .try_map(Self::try_from)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_node(node_id: Uuid, db: &PgPool) -> Result<Self> {
        sqlx::query("SELECT hosts.* FROM nodes INNER JOIN hosts ON nodes.host_id = hosts.id WHERE nodes.id = $1")
            .bind(node_id)
            .try_map(Self::try_from)
            .fetch_one(db)
            .await
            .map_err(Into::into)
    }

    pub async fn find_by_org(org_id: Uuid, db: &PgPool) -> Result<Vec<Self>> {
        let hosts = sqlx::query("SELECT * FROM hosts where org_id = $1 order by lower(name)")
            .bind(org_id)
            .try_map(Self::try_from)
            .fetch_all(db)
            .await?;
        let mut hosts_with_nodes: Vec<Host> = Vec::with_capacity(hosts.len());

        for mut host in hosts {
            let nodes = Node::find_all_by_host(host.id, db).await?;
            host.nodes = Some(nodes);

            hosts_with_nodes.push(host);
        }

        Ok(hosts_with_nodes)
    }

    pub async fn find_by_org_paginated(
        org_id: Uuid,
        limit: i32,
        offset: i32,
        db: &PgPool,
    ) -> Result<Vec<Self>> {
        let hosts = sqlx::query(
            "SELECT * FROM hosts where org_id = $1 order by lower(name) LIMIT $2 OFFSET $3",
        )
        .bind(org_id)
        .bind(limit)
        .bind(offset)
        .try_map(Self::try_from)
        .fetch_all(db)
        .await?;
        let mut hosts_with_nodes: Vec<Host> = Vec::with_capacity(hosts.len());

        for mut host in hosts {
            let nodes = Node::find_all_by_host(host.id, db).await?;
            host.nodes = Some(nodes);

            hosts_with_nodes.push(host);
        }

        Ok(hosts_with_nodes)
    }

    pub async fn create(req: HostRequest, db: &PgPool) -> Result<Self> {
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

        let mut tx = db.begin().await?;
        let mut host = sqlx::query(
            r#"INSERT INTO hosts 
            (
                name,
                version,
                location,
                ip_addr,
                val_ip_addrs,
                status,
                org_id,
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
            ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) RETURNING *"#,
        )
        .bind(req.name)
        .bind(req.version)
        .bind(req.location)
        .bind(req.ip_addr)
        .bind(req.val_ip_addrs)
        .bind(req.status)
        .bind(req.org_id)
        .bind(req.cpu_count)
        .bind(req.mem_size)
        .bind(req.disk_size)
        .bind(req.os)
        .bind(req.os_version)
        .bind(req.ip_gateway)
        .bind(req.ip_range_from)
        .bind(req.ip_range_to)
        .try_map(Self::try_from)
        .fetch_one(&mut tx)
        .await?;

        let mut vals: Vec<Validator> = vec![];

        // Create and add validators
        for ip in host.validator_ips() {
            // TODO: Refactor this
            let val = ValidatorRequest::new(host.id, &ip);

            let val = Validator::create_tx(val, &mut tx).await?;
            vals.push(val.to_owned());
        }

        host.validators = Some(vals);

        tx.commit().await?;

        // Create IP range for new host
        let req = IpAddressRangeRequest::try_new(
            host.ip_range_from.ok_or_else(required("ip.range.from"))?,
            host.ip_range_to.ok_or_else(required("ip.range.to"))?,
            Some(host.id),
        )?;
        IpAddress::create_range(req, db).await?;

        Ok(host)
    }

    #[deprecated(since = "0.2.0", note = "deprecated in favor of 'update_all'")]
    pub async fn update(id: Uuid, host: HostRequest, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await?;
        let host = sqlx::query(
            r#"UPDATE hosts SET name = $1, version = $2, location = $3, ip_addr = $4, status = $6, org_id = $7, cpu_count = $8, mem_size = $9, disk_size = $10, os = $11, os_version = $12 WHERE id = $13 RETURNING *"#
        )
        .bind(host.name)
        .bind(host.version)
        .bind(host.location)
        .bind(host.ip_addr)
        .bind(host.status)
        .bind(host.org_id)
        .bind(host.cpu_count)
        .bind(host.mem_size)
        .bind(host.disk_size)
        .bind(host.os)
        .bind(host.os_version)
        .bind(id)
        .try_map(Self::try_from)
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await?;
        Ok(host)
    }

    pub async fn update_all(fields: HostSelectiveUpdate, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await?;
        let host = sqlx::query(
            r#"UPDATE hosts SET 
                    org_id = COALESCE($1, org_id),
                    name = COALESCE($2, name),
                    version = COALESCE($3, version),
                    location = COALESCE($4, location),
                    cpu_count = COALESCE($5, cpu_count),
                    mem_size = COALESCE($6, mem_size),
                    disk_size = COALESCE($7, disk_size),
                    os = COALESCE($8, os),
                    os_version = COALESCE($9, os_version),
                    ip_addr = COALESCE($10, ip_addr),
                    val_ip_addrs = COALESCE($11, val_ip_addrs),
                    status = COALESCE($12, status)
                WHERE id = $13 RETURNING *"#,
        )
        .bind(fields.org_id)
        .bind(fields.name)
        .bind(fields.version)
        .bind(fields.location)
        .bind(fields.cpu_count)
        .bind(fields.mem_size)
        .bind(fields.disk_size)
        .bind(fields.os)
        .bind(fields.os_version)
        .bind(fields.ip_addr)
        .bind(fields.val_ip_addrs)
        .bind(fields.status)
        .bind(fields.id)
        .try_map(Self::try_from)
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await?;

        Ok(host)
    }

    pub async fn update_status(id: Uuid, host: HostStatusRequest, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await?;
        let host =
            sqlx::query(r#"UPDATE hosts SET version = $1, status = $2  WHERE id = $3 RETURNING *"#)
                .bind(host.version)
                .bind(host.status)
                .bind(id)
                .try_map(Self::try_from)
                .fetch_one(&mut tx)
                .await?;

        tx.commit().await?;
        Ok(host)
    }

    pub async fn delete(id: Uuid, db: &PgPool) -> Result<u64> {
        let mut tx = db.begin().await?;
        let deleted = sqlx::query("DELETE FROM hosts WHERE id = $1")
            .bind(id)
            .execute(&mut tx)
            .await?;

        tx.commit().await?;
        Ok(deleted.rows_affected())
    }

    pub fn validator_ips(&self) -> Vec<String> {
        match &self.val_ip_addrs {
            Some(s) => s.split(',').map(|ip| ip.trim().to_string()).collect(),
            None => vec![],
        }
    }
}

#[axum::async_trait]
impl FindableById for Host {
    async fn find_by_id(id: Uuid, db: &PgPool) -> Result<Self> {
        let mut host = sqlx::query("SELECT * FROM hosts WHERE id = $1")
            .bind(id)
            .try_map(Self::try_from)
            .fetch_one(db)
            .await?;

        // Add Validators list
        host.validators = Some(Validator::find_all_by_host(host.id, db).await?);
        // Add Nodes
        host.nodes = Some(Node::find_all_by_host(host.id, db).await?);

        Ok(host)
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
    async fn update_info(info: HostInfo, db: &PgPool) -> Result<Host> {
        let id: Uuid = info.id.ok_or_else(required("info.id"))?.parse()?;
        let mut tx = db.begin().await?;
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
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await?;

        Ok(host.try_into()?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostRequest {
    pub org_id: Option<Uuid>,
    pub name: String,
    pub version: Option<String>,
    pub location: Option<String>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub ip_addr: String,
    pub val_ip_addrs: Option<String>,
    pub status: ConnectionStatus,
    pub ip_range_from: Option<IpAddr>,
    pub ip_range_to: Option<IpAddr>,
    pub ip_gateway: Option<IpAddr>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HostSelectiveUpdate {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub location: Option<String>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub ip_addr: Option<String>,
    pub val_ip_addrs: Option<String>,
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
    pub async fn update_metrics(updates: Vec<Self>, db: &PgPool) -> Result<()> {
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
        query.execute(db).await?;
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
            org_id: host.org_id,
            name: host.name,
            version: host.version,
            location: host.location,
            cpu_count: host.cpu_count,
            mem_size: host.mem_size,
            disk_size: host.disk_size,
            os: host.os,
            os_version: host.os_version,
            ip_addr: host.ip_addr,
            val_ip_addrs: host.val_ip_addrs,
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
            org_id: None,
            name: info.name,
            version: info.version,
            location: info.location,
            cpu_count: info.cpu_count,
            mem_size: info.mem_size,
            disk_size: info.disk_size,
            os: info.os,
            os_version: info.os_version,
            ip_addr: info.ip,
            val_ip_addrs: None,
            status: None,
            ip_range_from: info.ip_range_from.map(|ip| ip.parse()).transpose()?,
            ip_range_to: info.ip_range_to.map(|ip| ip.parse()).transpose()?,
            ip_gateway: info.ip_gateway.map(|ip| ip.parse()).transpose()?,
            ..Default::default()
        };
        Ok(update)
    }
}

impl TryFrom<crate::grpc::blockjoy::ProvisionHostRequest> for HostCreateRequest {
    type Error = ApiError;

    fn try_from(request: crate::grpc::blockjoy::ProvisionHostRequest) -> Result<Self> {
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
            org_id: None,
            name: host_info.name.ok_or_else(required("info.name"))?,
            version: host_info.version,
            location: host_info.location,
            cpu_count: host_info.cpu_count,
            mem_size: host_info.mem_size,
            disk_size: host_info.disk_size,
            os: host_info.os,
            os_version: host_info.os_version,
            ip_addr: host_info.ip.ok_or_else(required("info.ip"))?,
            val_ip_addrs: None,
            ip_range_from,
            ip_range_to,
            ip_gateway,
        };
        Ok(req)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostCreateRequest {
    pub org_id: Option<Uuid>,
    pub name: String,
    pub version: Option<String>,
    pub location: Option<String>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub ip_addr: String,
    pub val_ip_addrs: Option<String>,
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
    pub org_id: Uuid,
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
    pub async fn create(req: HostProvisionRequest, db: &PgPool) -> Result<HostProvision> {
        let nodes_str = serde_json::to_string(&req.nodes)
            .map_err(|_| ApiError::from(anyhow::anyhow!("Couldn't parse nodes")))?;

        let mut host_provision = sqlx::query_as::<_, HostProvision>(
            r#"INSERT INTO host_provisions (id, org_id, nodes, ip_range_from, ip_range_to, ip_gateway) 
                   values ($1, $2, $3, $4, $5, $6) RETURNING *"#,
        )
        .bind(Self::generate_token())
        .bind(req.org_id)
        .bind(nodes_str)
        .bind(req.ip_range_from)
        .bind(req.ip_range_to)
        .bind(req.ip_gateway)
        .fetch_one(db)
        .await?;

        host_provision.set_install_cmd();

        Ok(host_provision)
    }

    pub async fn find_by_id(host_provision_id: &str, db: &PgPool) -> Result<HostProvision> {
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
        request: crate::grpc::blockjoy::ProvisionHostRequest,
        db: &PgPool,
    ) -> Result<Host> {
        let request = HostCreateRequest::try_from(request)?;

        HostProvision::claim(otp, request, db).await
    }

    pub async fn claim(
        host_provision_id: &str,
        mut req: HostCreateRequest,
        db: &PgPool,
    ) -> Result<Host> {
        let host_provision = Self::find_by_id(host_provision_id, db).await?;

        if host_provision.is_claimed() {
            return Err(anyhow::anyhow!("Host provision has already been claimed.").into());
        }

        req.org_id = Some(host_provision.org_id);
        req.val_ip_addrs = None;
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
        req.name = petname::petname(4, "_");

        //TODO: transaction this
        let mut host = Host::create(req.into(), db).await?;

        sqlx::query("UPDATE host_provisions set claimed_at = now(), host_id = $1 where id = $2")
            .bind(host.id)
            .bind(host_provision.id)
            .execute(db)
            .await?;

        host.nodes = Some(vec![]);

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
    pub org_id: Uuid,
    pub nodes: Option<Vec<NodeProvision>>,
    pub ip_range_from: IpAddr,
    pub ip_range_to: IpAddr,
    pub ip_gateway: IpAddr,
}
