use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy::NodeInfo as GrpcNodeInfo;
use crate::models::{command::HostCmd, validator::Validator, UpdateInfo};
use crate::server::DbPool;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgHasArrayType, PgRow};
use sqlx::{FromRow, PgPool, Row};
use uuid::Uuid;

/// NodeType reflects blockjoy.api.v1.node.NodeType in node.proto
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_node_type", rename_all = "snake_case")]
pub enum NodeType {
    Undefined,
    Api,
    Etl,
    Miner,
    Node,
    Oracle,
    Relay,
    Validator,
}

impl From<i32> for NodeType {
    fn from(ty: i32) -> Self {
        match ty {
            0 => Self::Undefined,
            1 => Self::Api,
            2 => Self::Etl,
            3 => Self::Miner,
            4 => Self::Node,
            5 => Self::Oracle,
            6 => Self::Relay,
            7 => Self::Validator,
            _ => Self::Undefined,
        }
    }
}

impl PgHasArrayType for NodeType {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("_enum_node_type")
    }
}

/// ContainerStatus reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_container_status", rename_all = "snake_case")]
pub enum ContainerStatus {
    Unknown,
    Creating,
    Running,
    Starting,
    Stopping,
    Stopped,
    Upgrading,
    Upgraded,
    Deleting,
    Deleted,
    Installing,
    Snapshotting,
}

/// NodeSyncStatus reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_node_sync_status", rename_all = "snake_case")]
pub enum NodeSyncStatus {
    Unknown,
    Syncing,
    Synced,
}

/// NodeStakingStatus reflects blockjoy.api.v1.node.NodeInfo.StakingStatus in node.proto
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_node_staking_status", rename_all = "snake_case")]
pub enum NodeStakingStatus {
    Unknown,
    Follower,
    Staked,
    Staking,
    Validating,
    Consensus,
    Unstaked,
}

/// NodeChainStatus reflects blockjoy.api.v1.node.NodeInfo.ApplicationStatus in node.proto
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_node_chain_status", rename_all = "snake_case")]
pub enum NodeChainStatus {
    Unknown,
    // Staking states
    Follower,
    Staked,
    Staking,
    Validating,
    Consensus,
    // General chain states
    Broadcasting,
    Cancelled,
    Delegating,
    Delinquent,
    Disabled,
    Earning,
    Electing,
    Elected,
    Exporting,
    Ingesting,
    Mining,
    Minting,
    Processing,
    Relaying,
    Removed,
    Removing,
}

impl From<i32> for NodeChainStatus {
    fn from(status: i32) -> Self {
        match status {
            0 => Self::Unknown,
            1 => Self::Follower,
            2 => Self::Staked,
            3 => Self::Staking,
            4 => Self::Validating,
            5 => Self::Consensus,
            6 => Self::Broadcasting,
            7 => Self::Cancelled,
            8 => Self::Delegating,
            9 => Self::Delinquent,
            10 => Self::Disabled,
            11 => Self::Earning,
            12 => Self::Electing,
            13 => Self::Elected,
            14 => Self::Exporting,
            15 => Self::Ingesting,
            16 => Self::Mining,
            17 => Self::Minting,
            18 => Self::Processing,
            19 => Self::Relaying,
            20 => Self::Removed,
            21 => Self::Removing,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
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
    pub sync_status: NodeSyncStatus,
    pub chain_status: NodeChainStatus,
    pub staking_status: NodeStakingStatus,
    pub container_status: ContainerStatus,
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
            r#"INSERT INTO nodes (
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
                    chain_status,
                    sync_status
                ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING *"#,
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
        .bind(&req.chain_status)
        .bind(&req.sync_status)
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
        sqlx::query_as::<_, Node>(
            r#"UPDATE nodes SET 
                    version = COALESCE($1, version),
                    ip_addr = COALESCE($2, ip_addr),
                    block_height = COALESCE($3, block_height),
                    node_data = COALESCE($4, node_data),
                    chain_status = COALESCE($5, chain_status),
                    sync_status = COALESCE($6, sync_status)
                WHERE id = $7 RETURNING *"#,
        )
        .bind(&info.version)
        .bind(&info.ip_addr)
        .bind(&info.block_height)
        .bind(&info.node_data)
        .bind(&info.chain_status)
        .bind(&info.sync_status)
        .bind(&id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_all_by_host(host_id: Uuid, db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query_as::<_, Self>("SELECT * FROM nodes WHERE host_id = $1 order by name DESC")
            .bind(host_id)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn running_nodes_count(db: &PgPool) -> Result<i32> {
        match sqlx::query(
            r#"select count(id)::int from nodes where chain_status in
                                 (
                                  'broadcasting'::enum_node_chain_status,
                                  'cancelled'::enum_node_chain_status,
                                  'delegating'::enum_node_chain_status,
                                  'delinquent'::enum_node_chain_status,
                                  'earning'::enum_node_chain_status,
                                  'electing'::enum_node_chain_status,
                                  'elected'::enum_node_chain_status,
                                  'exported'::enum_node_chain_status,
                                  'ingesting'::enum_node_chain_status,
                                  'mining'::enum_node_chain_status,
                                  'minting'::enum_node_chain_status,
                                  'processing'::enum_node_chain_status,
                                  'relaying'::enum_node_chain_status
                                 );"#,
        )
        .fetch_one(db)
        .await
        {
            Ok(row) => Ok(row.get(0)),
            Err(e) => {
                tracing::error!("Got error while retrieving number of running hosts: {}", e);
                Err(ApiError::from(e))
            }
        }
    }

    pub async fn halted_nodes_count(db: &PgPool) -> Result<i32> {
        match sqlx::query(
            r#"select count(id)::int from nodes where chain_status in
                                 (
                                  'unknown'::enum_node_chain_status,
                                  'disabled'::enum_node_chain_status,
                                  'removed'::enum_node_chain_status,
                                  'removing'::enum_node_chain_status
                                 );"#,
        )
        .fetch_one(db)
        .await
        {
            Ok(row) => Ok(row.get(0)),
            Err(e) => {
                tracing::error!("Got error while retrieving number of running hosts: {}", e);
                Err(ApiError::from(e))
            }
        }
    }
}

#[tonic::async_trait]
impl UpdateInfo<GrpcNodeInfo, Node> for Node {
    async fn update_info(info: GrpcNodeInfo, db: DbPool) -> Result<Node> {
        let id = Uuid::from(info.id.unwrap());
        let mut tx = db.begin().await?;
        let node = sqlx::query_as::<_, Node>(
            r##"UPDATE nodes SET
                         name = COALESCE($1, name),
                         ip_addr = COALESCE($2, ip_addr),
                         chain_status = COALESCE($3, chain_status),
                         sync_status = COALESCE($4, sync_status),
                         staking_status = COALESCE($5, staking_status),
                         block_height = COALESCE($6, block_height)
                WHERE id = $7
                RETURNING *
            "##,
        )
        .bind(info.name)
        .bind(info.ip)
        .bind(info.app_status.as_ref().unwrap())
        .bind(info.sync_status.as_ref().unwrap())
        .bind(info.staking_status.as_ref().unwrap())
        .bind(info.block_height)
        .bind(id)
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await?;

        Ok(node)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeProvision {
    pub blockchain_id: Uuid,
    pub node_type: NodeType,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeCreateRequest {
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
    pub chain_status: NodeChainStatus,
    pub sync_status: NodeSyncStatus,
    pub staking_status: Option<NodeStakingStatus>,
    pub container_status: ContainerStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub version: Option<String>,
    pub ip_addr: Option<String>,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub chain_status: Option<NodeChainStatus>,
    pub sync_status: Option<NodeSyncStatus>,
    pub staking_status: Option<NodeStakingStatus>,
    pub container_status: Option<ContainerStatus>,
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
