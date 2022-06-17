use super::{HostCmd, Validator};
use crate::errors::{ApiError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{FromRow, PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_node_type", rename_all = "snake_case")]
pub enum NodeType {
    Api,
    Etl,
    Node,
    Oracle,
    Relay,
    Validator,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_node_status", rename_all = "snake_case")]
pub enum NodeStatus {
    Available,
    Broadcasting,
    Cancelled,
    Consensus,
    Creating,
    Delegating,
    Delinquent,
    Disabled,
    Earning,
    Electing,
    Elected,
    Exporting,
    Ingesting,
    Installing,
    Migrating,
    Mining,
    Minting,
    Processing,
    Relaying,
    Removed,
    Removing,
    Running,
    Snapshoting,
    Staked,
    Staking,
    Started,
    Starting,
    Stopped,
    Stopping,
    Synced,
    Syncing,
    Upgrading,
    Validating,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Node {
    pub id: Uuid,
    pub org_id: Uuid,
    pub host_id: Uuid,
    pub name: Option<String>,
    pub groups: Option<String>,
    pub version: Option<String>,
    pub ip_addr: Option<String>,
    pub blockchain_id: Uuid,
    pub node_type: NodeType,
    pub address: Option<String>,
    pub wallet_address: Option<String>,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: NodeStatus,
    pub is_online: bool,
}

impl Node {
    pub async fn find_by_id(id: &Uuid, db: &PgPool) -> Result<Node> {
        sqlx::query_as::<_, Node>("SELECT * FROM nodes where id = $1")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn create(req: &NodeCreateRequest, db: &PgPool) -> Result<Node> {
        let mut tx = db.begin().await?;
        let node = sqlx::query_as::<_, Node>(
            r##"INSERT INTO nodes (
                    org_id, 
                    host_id,
                    name, 
                    groups, 
                    version, 
                    ip_addr, 
                    blockchain_id, 
                    node_type, 
                    address, 
                    wallet_address, 
                    block_height, 
                    node_data,
                    status,
                    is_online
                ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING *"##,
        )
        .bind(&req.org_id)
        .bind(&req.host_id)
        .bind(&req.name)
        .bind(&req.groups)
        .bind(&req.version)
        .bind(&req.ip_addr)
        .bind(&req.blockchain_id)
        .bind(&req.node_type)
        .bind(&req.address)
        .bind(&req.wallet_address)
        .bind(&req.block_height)
        .bind(&req.node_data)
        .bind(&req.status)
        .bind(&req.is_online)
        .fetch_one(&mut tx)
        .await
        .map_err(ApiError::from)?;

        let node_info = serde_json::json!({"node_id": &node.id});

        //TODO: Move this to commands
        sqlx::query("INSERT INTO commands (host_id, cmd, sub_cmd) values ($1,$2,$3)")
            .bind(&req.host_id)
            .bind(HostCmd::CreateNode)
            .bind(node_info)
            .execute(&mut tx)
            .await
            .map_err(ApiError::from)?;

        tx.commit().await?;

        Ok(node)
    }

    pub async fn update_info(id: &Uuid, info: &NodeInfo, db: &PgPool) -> Result<Node> {
        sqlx::query_as::<_, Node>("UPDATE nodes SET version=$1, ip_addr=$2, block_height=$3, node_data=$4, status=$5, is_online-$6 WHERE id=$7 RETURNING *")
        .bind(&info.version)
        .bind(&info.ip_addr)
        .bind(&info.block_height)
        .bind(&info.node_data)
        .bind(&info.status)
        .bind(&info.is_online)
        .bind(&id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeCreateRequest {
    org_id: Uuid,
    host_id: Uuid,
    name: Option<String>,
    groups: Option<String>,
    version: Option<String>,
    ip_addr: Option<String>,
    blockchain_id: Uuid,
    node_type: NodeType,
    address: Option<String>,
    wallet_address: Option<String>,
    block_height: Option<i64>,
    node_data: Option<serde_json::Value>,
    status: NodeStatus,
    is_online: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    version: Option<String>,
    ip_addr: Option<String>,
    block_height: Option<i64>,
    node_data: Option<serde_json::Value>,
    status: NodeStatus,
    is_online: bool,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct NodeGroup {
    id: Uuid,
    name: String,
    node_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    nodes: Option<Vec<Validator>>,
}

impl NodeGroup {
    pub async fn find_all(db: &PgPool) -> Result<Vec<NodeGroup>> {
        sqlx::query("SELECT user_id as id, users.email as name, count(*) as node_count, null as nodes FROM validators INNER JOIN users on users.id = validators.user_id  GROUP BY user_id, users.email ORDER BY users.email DESC")
            .map(Self::from)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_id(db: &PgPool, id: Uuid) -> Result<NodeGroup> {
        let validators = Validator::find_all_by_user(id, db).await?;
        let name = validators.first().unwrap().name.clone();
        Ok(NodeGroup {
            id,
            name,
            node_count: validators.len() as i64,
            nodes: Some(validators),
        })
    }
}

impl From<PgRow> for NodeGroup {
    fn from(row: PgRow) -> Self {
        NodeGroup {
            id: row
                .try_get("id")
                .expect("Couldn't try_get id for node_group."),
            name: row
                .try_get("name")
                .expect("Couldn't try_get name node_group."),
            node_count: row
                .try_get("node_count")
                .expect("Couldn't try_get node_count node_group."),
            nodes: None,
        }
    }
}
