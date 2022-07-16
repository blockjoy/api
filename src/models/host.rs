use super::{Node, NodeCreateRequest, NodeProvision, NodeStatus, Validator, ValidatorRequest};
use crate::errors::{ApiError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{FromRow, PgPool, Row};
use std::convert::From;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, sqlx::Type)]
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
    pub token: String,
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
            token: row
                .try_get("token")
                .expect("Couldn't try_get token for host."),
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

impl casbin_authorization::auth::Authorizable for Host {
    fn get_role(&self) -> String {
        "host".to_string()
    }
}

impl Host {
    pub async fn find_all(db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query("SELECT * FROM hosts order by lower(name)")
            .map(Self::from)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_id(id: Uuid, db: &PgPool) -> Result<Self> {
        let mut host = sqlx::query("SELECT * FROM hosts WHERE id = $1")
            .bind(id)
            .map(Self::from)
            .fetch_one(db)
            .await?;

        // Add Validators list
        host.validators = Some(Validator::find_all_by_host(host.id, db).await?);

        Ok(host)
    }

    pub async fn find_by_token(token: &str, db: &PgPool) -> Result<Self> {
        let mut host = sqlx::query("SELECT * FROM hosts WHERE token = $1")
            .bind(token)
            .map(Self::from)
            .fetch_one(db)
            .await?;

        // Add Validators list
        host.validators = Some(Validator::find_all_by_host(host.id, db).await?);
        host.nodes = Some(Node::find_all_by_host(host.id, db).await?);

        Ok(host)
    }

    pub async fn create(req: HostRequest, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await?;
        let mut host = sqlx::query("INSERT INTO hosts (name, version, location, ip_addr, val_ip_addrs, token, status, org_id, cpu_count, mem_size, disk_size, os, os_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING *")
        .bind(req.name)
        .bind(req.version)
        .bind(req.location)
        .bind(req.ip_addr)
        .bind(req.val_ip_addrs)
        .bind(req.token)
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

        Ok(host)
    }

    pub async fn update(id: Uuid, host: HostRequest, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await.unwrap();
        let host = sqlx::query(
            r#"UPDATE hosts SET name = $1, version = $2, location = $3, ip_addr = $4, token = $5, status = $6, org_id = $7, cpu_count = $8, mem_size = $9, disk_size = $10, os = $11, os_version = $12 WHERE id = $13 RETURNING *"#
        )
        .bind(host.name)
        .bind(host.version)
        .bind(host.location)
        .bind(host.ip_addr)
        .bind(host.token)
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
        let deleted = sqlx::query("DELETE FROM hosts WHERE id = $1")
            .bind(id)
            .execute(&mut tx)
            .await?;

        tx.commit().await?;
        Ok(deleted.rows_affected())
    }

    pub fn new_token() -> String {
        Uuid::new_v4()
            .to_simple()
            .encode_lower(&mut Uuid::encode_buffer())
            .to_string()
    }

    pub fn validator_ips(&self) -> Vec<String> {
        match &self.val_ip_addrs {
            Some(s) => s.split(',').map(|ip| ip.trim().to_string()).collect(),
            None => vec![],
        }
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
    pub token: String, //TODO: Protect this
    pub status: ConnectionStatus,
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
            token: Host::new_token(),
            status: ConnectionStatus::Offline,
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
                    status: NodeStatus::Creating,
                    is_online: false,
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
