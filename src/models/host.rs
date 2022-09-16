use super::{
    validator::Validator, validator::ValidatorRequest, Node, NodeChainStatus, NodeCreateRequest,
    NodeProvision, NodeSyncStatus, Token, TokenRole,
};
use crate::auth::{FindableById, Owned, TokenHolderType, TokenIdentifyable, TokenType};
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy::HostInfo;
use crate::models::{ContainerStatus, UpdateInfo};
use crate::server::DbPool;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{FromRow, PgPool, Row};
use std::convert::From;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_conn_status", rename_all = "snake_case")]
pub enum ConnectionStatus {
    Online,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl From<PgRow> for Host {
    fn from(row: PgRow) -> Self {
        Host {
            id: row.try_get("id").expect("Couldn't try_get id for host."),
            org_id: row
                .try_get("org_id")
                .expect("Couldn't try_get org_id for host."),
            name: row
                .try_get("name")
                .expect("Couldn't try_get name for host."),
            cpu_count: row
                .try_get("cpu_count")
                .expect("Couldn't try_get cpu_count for host."),
            mem_size: row
                .try_get("mem_size")
                .expect("Couldn't try_get mem_size for host."),
            disk_size: row
                .try_get("disk_size")
                .expect("Couldn't try_get cpu_count for host."),
            os: row.try_get("os").expect("Couldn't try_get os for host."),
            os_version: row
                .try_get("os_version")
                .expect("Couldn't try_get os_version for host."),
            version: row
                .try_get("version")
                .expect("Couldn't try_get version for host."),
            location: row
                .try_get("location")
                .expect("Couldn't try_get location for host."),
            ip_addr: row
                .try_get("ip_addr")
                .expect("Couldn't try_get ip_addr for host."),
            val_ip_addrs: row
                .try_get("val_ip_addrs")
                .expect("Couldn't try_get val_ip_addrs for host."),
            status: row
                .try_get("status")
                .expect("Couldn't try_get status for host."),
            validators: None,
            nodes: None,
            created_at: row
                .try_get("created_at")
                .expect("Couldn't try_get created_at for host."),
        }
    }
}

impl Host {
    pub async fn toggle_online(id: Uuid, is_online: bool, db: &PgPool) -> Result<()> {
        let status = if is_online {
            ConnectionStatus::Online
        } else {
            ConnectionStatus::Offline
        };

        sqlx::query("UPDATE hosts set status = $1 where id = $2")
            .bind(status)
            .bind(id)
            .fetch_one(db)
            .await?;

        Ok(())
    }

    pub async fn find_all(db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query("SELECT * FROM hosts order by lower(name)")
            .map(Self::from)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_org(org_id: Uuid, db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query("SELECT * FROM hosts where org_id = $1 order by lower(name)")
            .bind(org_id)
            .map(Self::from)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_org_paginated(
        org_id: Uuid,
        limit: i32,
        offset: i32,
        db: &PgPool,
    ) -> Result<Vec<Self>> {
        sqlx::query("SELECT * FROM hosts where org_id = $1 order by lower(name) LIMIT $2 OFFSET $3")
            .bind(org_id)
            .bind(limit)
            .bind(offset)
            .map(Self::from)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn create(req: HostRequest, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await?;
        let mut host = sqlx::query("INSERT INTO hosts (name, version, location, ip_addr, val_ip_addrs, status, org_id, cpu_count, mem_size, disk_size, os, os_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING *")
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
        .map(|row: PgRow| {
            Self::from(row)
        })
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

        Token::create_for::<Host>(&host, TokenRole::Service, TokenType::Login, db).await?;

        Ok(host)
    }

    #[deprecated(since = "0.2.0", note = "deprecated in favor of 'update_all'")]
    pub async fn update(id: Uuid, host: HostRequest, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await.unwrap();
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
        .map(|row: PgRow| {
            Self::from(row)
        })
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await.unwrap();
        Ok(host)
    }

    pub async fn update_all(id: Uuid, fields: HostSelectiveUpdate, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await.unwrap();
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
        .bind(id)
        .map(Self::from)
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await.unwrap();

        Ok(host)
    }

    pub async fn update_status(id: Uuid, host: HostStatusRequest, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await.unwrap();
        let host =
            sqlx::query(r#"UPDATE hosts SET version = $1, status = $2  WHERE id = $3 RETURNING *"#)
                .bind(host.version)
                .bind(host.status)
                .bind(id)
                .map(Self::from)
                .fetch_one(&mut tx)
                .await?;

        tx.commit().await.unwrap();
        Ok(host)
    }

    pub async fn delete(id: Uuid, db: &PgPool) -> Result<u64> {
        let mut tx = db.begin().await?;
        // TODO: cascading delete doesn't seem to work, so i'm manually deleting the token
        let token_deleted = sqlx::query("delete from tokens where host_id = $1")
            .bind(id)
            .execute(&mut tx)
            .await?;
        // ////
        let deleted = sqlx::query("DELETE FROM hosts WHERE id = $1")
            .bind(id)
            .execute(&mut tx)
            .await?;

        tx.commit().await?;
        Ok(deleted.rows_affected() + token_deleted.rows_affected())
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
            .map(Self::from)
            .fetch_one(db)
            .await?;

        // Add Validators list
        host.validators = Some(Validator::find_all_by_host(host.id, db).await?);

        Ok(host)
    }
}

#[axum::async_trait]
impl TokenIdentifyable for Host {
    async fn set_token(token_id: Uuid, host_id: Uuid, db: &PgPool) -> Result<()> {
        let host_token = super::HostToken::new(host_id, token_id, TokenType::Login);
        host_token.create_or_update(db).await?;
        Ok(())
    }

    fn get_holder_type() -> TokenHolderType {
        TokenHolderType::Host
    }

    fn get_id(&self) -> Uuid {
        self.id
    }

    async fn delete_token(host_id: Uuid, db: &PgPool) -> Result<()> {
        super::HostToken::delete_by_host(host_id, TokenType::Login, db).await?;
        Ok(())
    }

    async fn get_token(&self, db: &PgPool) -> Result<Token> {
        Token::get::<Host>(self.id, TokenType::Login, db).await
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
    async fn update_info(info: HostInfo, db: DbPool) -> Result<Host> {
        let id = Uuid::from(info.id.unwrap());
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
        .map(Self::from)
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await?;

        Ok(host)
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
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HostSelectiveUpdate {
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
        }
    }
}

impl From<HostInfo> for HostSelectiveUpdate {
    fn from(info: HostInfo) -> Self {
        Self {
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
        }
    }
}

impl From<crate::grpc::blockjoy::ProvisionHostRequest> for HostCreateRequest {
    fn from(request: crate::grpc::blockjoy::ProvisionHostRequest) -> Self {
        let host_info = request.info.unwrap();

        Self {
            org_id: None,
            name: host_info.name.unwrap(),
            version: host_info.version,
            location: host_info.location,
            cpu_count: host_info.cpu_count,
            mem_size: host_info.mem_size,
            disk_size: host_info.disk_size,
            os: host_info.os,
            os_version: host_info.os_version,
            ip_addr: host_info.ip.unwrap(),
            val_ip_addrs: Some(request.validator_ips.join(",")),
        }
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
}

impl HostProvision {
    pub async fn create(req: HostProvisionRequest, db: &PgPool) -> Result<HostProvision> {
        let nodes_str = serde_json::to_string(&req.nodes)
            .map_err(|_| ApiError::from(anyhow::anyhow!("Couldn't parse nodes")))?;

        let mut host_provision = sqlx::query_as::<_, HostProvision>(
            "INSERT INTO host_provisions (id, org_id, nodes) values ($1, $2, $3) RETURNING *",
        )
        .bind(Self::generate_token())
        .bind(req.org_id)
        .bind(nodes_str)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)?;

        host_provision.set_install_cmd();

        Ok(host_provision)
    }

    pub async fn find_by_id(host_provision_id: &str, db: &PgPool) -> Result<HostProvision> {
        let mut host_provision =
            sqlx::query_as::<_, HostProvision>("SELECT * FROM host_provisions where id = $1")
                .bind(host_provision_id)
                .fetch_one(db)
                .await
                .map_err(ApiError::from)?;
        host_provision.set_install_cmd();

        Ok(host_provision)
    }

    /// Wrapper for HostProvision::claim, taking ProvisionHostRequest received via gRPC instead of HostCreateRequest
    pub async fn claim_by_grpc_provision(
        otp: &str,
        request: crate::grpc::blockjoy::ProvisionHostRequest,
        db: &PgPool,
    ) -> Result<Host> {
        let request = HostCreateRequest::from(request);

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

        let node_provisions: Option<Vec<NodeProvision>> =
            serde_json::from_str(&host_provision.nodes.unwrap_or_default()).map_err(|_| {
                ApiError::UnexpectedError(anyhow::anyhow!("Couldn't parse nodes data"))
            })?;

        //TODO: transaction this
        let mut host = Host::create(req.into(), db).await?;

        sqlx::query("UPDATE host_provisions set claimed_at = now(), host_id = $1 where id = $2")
            .bind(host.id)
            .bind(host_provision.id)
            .execute(db)
            .await?;

        let mut host_nodes = vec![];

        if let Some(nodes) = node_provisions {
            for node in nodes {
                let n = NodeCreateRequest {
                    org_id: host_provision.org_id,
                    host_id: host.id,
                    //TODO: Clean this up
                    name: Some(petname::petname(3, "-")),
                    groups: None,
                    version: None,
                    ip_addr: None,
                    blockchain_id: node.blockchain_id,
                    node_type: node.node_type,
                    address: None,
                    wallet_address: None,
                    block_height: None,
                    node_data: None,
                    chain_status: NodeChainStatus::Unknown,
                    sync_status: NodeSyncStatus::Unknown,
                    staking_status: None,
                    container_status: ContainerStatus::Installing,
                };
                host_nodes.push(Node::create(&n, db).await?);
            }
        };

        host.nodes = Some(host_nodes);

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
}
