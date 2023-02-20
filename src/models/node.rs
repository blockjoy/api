use super::node_type::*;
use super::schema::nodes;
use crate::auth::FindableById;
use crate::cookbook::get_hw_requirements;
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy::NodeInfo as GrpcNodeInfo;
use crate::models::{Blockchain, Host, IpAddress, UpdateInfo};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use diesel::deserialize::FromSqlRow;
use diesel::expression::AsExpression;
use diesel::prelude::*;
use diesel::sql_types::VarChar;
use diesel::Queryable;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use diesel_enum::DbEnum;
use uuid::Uuid;

/// ContainerStatus reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow, DbEnum)]
#[diesel(sql_type = VarChar)]
#[diesel_enum(error_fn = ApiError::db_enum)]
#[diesel_enum(error_type = ApiError)]
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

impl TryFrom<i32> for ContainerStatus {
    type Error = ApiError;

    fn try_from(n: i32) -> Result<Self> {
        match n {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Creating),
            2 => Ok(Self::Running),
            3 => Ok(Self::Starting),
            4 => Ok(Self::Stopping),
            5 => Ok(Self::Stopped),
            6 => Ok(Self::Upgrading),
            7 => Ok(Self::Upgraded),
            8 => Ok(Self::Deleting),
            9 => Ok(Self::Deleted),
            10 => Ok(Self::Installing),
            11 => Ok(Self::Snapshotting),
            _ => Err(ApiError::UnexpectedError(anyhow!(
                "Cannot convert {n} to ContainerStatus"
            ))),
        }
    }
}

/// NodeSyncStatus reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow, DbEnum)]
#[diesel(sql_type = EnumNodeSyncStatus)]
#[diesel_enum(error_fn = ApiError::db_enum)]
#[diesel_enum(error_type = ApiError)]
pub enum NodeSyncStatus {
    Unknown,
    Syncing,
    Synced,
}

impl TryFrom<i32> for NodeSyncStatus {
    type Error = ApiError;

    fn try_from(n: i32) -> Result<Self> {
        match n {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Syncing),
            2 => Ok(Self::Synced),
            _ => Err(ApiError::UnexpectedError(anyhow!(
                "Cannot convert {n} to NodeSyncStatus"
            ))),
        }
    }
}

/// NodeStakingStatus reflects blockjoy.api.v1.node.NodeInfo.StakingStatus in node.proto
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow, DbEnum)]
#[diesel(sql_type = VarChar)]
#[diesel_enum(error_fn = ApiError::db_enum)]
#[diesel_enum(error_type = ApiError)]
pub enum NodeStakingStatus {
    Unknown,
    Follower,
    Staked,
    Staking,
    Validating,
    Consensus,
    Unstaked,
}

impl TryFrom<i32> for NodeStakingStatus {
    type Error = ApiError;

    fn try_from(n: i32) -> Result<Self> {
        match n {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Follower),
            2 => Ok(Self::Staked),
            3 => Ok(Self::Staking),
            4 => Ok(Self::Validating),
            5 => Ok(Self::Consensus),
            6 => Ok(Self::Unstaked),
            _ => Err(ApiError::UnexpectedError(anyhow!(
                "Cannot convert {n} to NodeStakingStatus"
            ))),
        }
    }
}

/// NodeChainStatus reflects blockjoy.api.v1.node.NodeInfo.ApplicationStatus in node.proto
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow, DbEnum)]
#[diesel(sql_type = VarChar)]
#[diesel_enum(error_fn = ApiError::db_enum)]
#[diesel_enum(error_type = ApiError)]
pub enum NodeChainStatus {
    Unknown,
    Provisioning,
    Broadcasting,
    Cancelled,
    Delegating,
    Delinquent,
    Disabled,
    Earning,
    Electing,
    Elected,
    Exported,
    Ingesting,
    Mining,
    Minting,
    Processing,
    Relaying,
    Removed,
    Removing,
}

impl TryFrom<i32> for NodeChainStatus {
    type Error = ApiError;

    fn try_from(n: i32) -> Result<Self> {
        match n {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Provisioning),
            2 => Ok(Self::Broadcasting),
            3 => Ok(Self::Cancelled),
            4 => Ok(Self::Delegating),
            5 => Ok(Self::Delinquent),
            6 => Ok(Self::Disabled),
            7 => Ok(Self::Earning),
            8 => Ok(Self::Electing),
            9 => Ok(Self::Elected),
            10 => Ok(Self::Exported),
            11 => Ok(Self::Ingesting),
            12 => Ok(Self::Mining),
            13 => Ok(Self::Minting),
            14 => Ok(Self::Processing),
            15 => Ok(Self::Relaying),
            16 => Ok(Self::Removed),
            17 => Ok(Self::Removing),
            _ => Err(ApiError::UnexpectedError(anyhow!(
                "Cannot convert {n} to NodeChainStatus"
            ))),
        }
    }
}

#[derive(Clone, Debug, Queryable, Identifiable)]
pub struct Node {
    pub id: Uuid,
    pub org_id: Uuid,
    pub host_id: Uuid,
    pub name: Option<String>,
    pub groups: Option<String>,
    pub version: Option<String>,
    pub ip_addr: Option<String>,
    pub ip_gateway: Option<String>,
    pub blockchain_id: Uuid,
    pub node_type: serde_json::Value,
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
    pub self_update: bool,
    pub block_age: Option<i64>,
    pub consensus: Option<bool>,
    pub vcpu_count: i64,
    pub mem_size_mb: i64,
    pub disk_size_gb: i64,
    pub host_name: String,
    pub network: String,
}

#[derive(Clone, Debug)]
pub struct NodeFilter {
    pub status: Vec<String>,
    pub node_types: Vec<String>,
    pub blockchains: Vec<uuid::Uuid>,
}

#[axum::async_trait]
impl FindableById for Node {
    async fn find_by_id(id: uuid::Uuid, db: &mut AsyncPgConnection) -> Result<Self> {
        let node = nodes::table.find(id).get_result(db).await?;
        Ok(node)
    }
}

impl Node {
    pub async fn create(req: &mut NodeCreateRequest, tx: &mut super::DbTrx<'_>) -> Result<Node> {
        let chain = Blockchain::find_by_id(req.blockchain_id, tx).await?;
        let node_type = NodeTypeKey::str_from_value(req.node_type.get_id());
        let requirements = get_hw_requirements(chain.name, node_type, req.version.clone()).await?;
        let host_id = Host::get_next_available_host_id(requirements, tx).await?;
        let host = Host::find_by_id(host_id, tx).await?;

        req.ip_gateway = host.ip_gateway.map(|ip| ip.to_string());
        req.ip_addr = Some(IpAddress::next_for_host(host_id, tx).await?.ip.to_string());

        diesel::insert_into(nodes::table)
            .values(req)
            .get_result(tx)
            .await
            .map_err(|e| {
                tracing::error!("Error creating node: {}", e);
                e.into()
            })
    }

    pub async fn find_all_by_host(host_id: Uuid, db: &mut AsyncPgConnection) -> Result<Vec<Self>> {
        sqlx::query_as("SELECT * FROM nodes WHERE host_id = $1 order by name DESC")
            .bind(host_id)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_all_by_org(
        org_id: Uuid,
        offset: i32,
        limit: i32,
        db: &mut AsyncPgConnection,
    ) -> Result<Vec<Self>> {
        sqlx::query_as::<_, Self>(
            r#"
            SELECT * FROM nodes WHERE org_id = $1 
            ORDER BY name DESC 
            OFFSET $2
            LIMIT $3"#,
        )
        .bind(org_id)
        .bind(offset)
        .bind(limit)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    // TODO: Check role if user is allowed to delete the node
    pub async fn belongs_to_user_org(
        org_id: Uuid,
        user_id: Uuid,
        db: &mut AsyncPgConnection,
    ) -> Result<bool> {
        let cnt: i32 = sqlx::query_scalar(
            r#"
            SELECT count(*)::int FROM orgs_users WHERE org_id = $1 and user_id = $2 
            "#,
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;

        Ok(cnt > 0)
    }

    pub async fn find_all_by_filter(
        org_id: Uuid,
        filter: NodeFilter,
        offset: i32,
        limit: i32,
        db: &mut AsyncPgConnection,
    ) -> Result<Vec<Self>> {
        let mut nodes = sqlx::query_as::<_, Self>(
            r#"
                SELECT * FROM nodes
                WHERE org_id = $1
                ORDER BY created_at DESC
                OFFSET $2
                LIMIT $3
            "#,
        )
        .bind(org_id)
        .bind(offset)
        .bind(limit)
        .fetch_all(db)
        .await?;

        // Apply filters if present
        if !filter.blockchains.is_empty() {
            tracing::debug!("Applying blockchain filter: {:?}", filter.blockchains);
            nodes.retain(|n| filter.blockchains.contains(&n.blockchain_id));
        }
        if !filter.status.is_empty() {
            nodes.retain(|n| filter.status.contains(&n.chain_status.to_string()));
        }
        if !filter.node_types.is_empty() {
            nodes.retain(|n| {
                filter
                    .node_types
                    .contains(&n.node_type.get_id().to_string())
            })
        }

        Ok(nodes)
    }

    pub async fn running_nodes_count(org_id: &Uuid, db: &mut AsyncPgConnection) -> Result<i32> {
        match sqlx::query(
            r#"select COALESCE(count(id)::int, 0) from nodes where chain_status in
                                 (
                                  'broadcasting'::enum_node_chain_status,
                                  'provisioning'::enum_node_chain_status,
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
                                 ) and org_id = $1;"#,
        )
        .bind(org_id)
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

    pub async fn halted_nodes_count(org_id: &Uuid, db: &mut AsyncPgConnection) -> Result<i32> {
        match sqlx::query(
            r#"select COALESCE(count(id)::int, 0) from nodes where chain_status in
                                 (
                                  'unknown'::enum_node_chain_status,
                                  'disabled'::enum_node_chain_status,
                                  'removed'::enum_node_chain_status,
                                  'removing'::enum_node_chain_status
                                 ) and org_id = $1;"#,
        )
        .bind(org_id)
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

    pub async fn delete(node_id: Uuid, tx: &mut super::DbTrx<'_>) -> Result<Self> {
        sqlx::query_as(r#"DELETE FROM nodes WHERE id = $1 RETURNING *"#)
            .bind(node_id)
            .fetch_one(tx)
            .await
            .map_err(ApiError::from)
    }
}

#[tonic::async_trait]
impl UpdateInfo<GrpcNodeInfo, Node> for Node {
    async fn update_info(info: GrpcNodeInfo, tx: &mut super::DbTrx<'_>) -> Result<Node> {
        let req: NodeUpdateRequest = info.try_into()?;
        let node: Node = sqlx::query_as(
            r##"UPDATE nodes SET
                         name = COALESCE($1, name),
                         ip_addr = COALESCE($2, ip_addr),
                         chain_status = COALESCE($3, chain_status),
                         sync_status = COALESCE($4, sync_status),
                         staking_status = COALESCE($5, staking_status),
                         block_height = COALESCE($6, block_height),
                         self_update = COALESCE($7, self_update),
                         address = COALESCE($8, address)
                WHERE id = $9
                RETURNING *
            "##,
        )
        .bind(req.name)
        .bind(req.ip_addr)
        .bind(req.chain_status)
        .bind(req.sync_status)
        .bind(req.staking_status)
        .bind(req.block_height)
        .bind(req.self_update)
        .bind(req.address)
        .bind(req.id)
        .fetch_one(tx)
        .await?;

        Ok(node)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeProvision {
    pub blockchain_id: Uuid,
    pub node_type: NodeType,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = nodes)]
pub struct NodeCreateRequest {
    pub org_id: Uuid,
    pub host_name: String,
    pub name: Option<String>,
    pub groups: Option<String>,
    pub version: Option<String>,
    pub ip_addr: Option<String>,
    pub ip_gateway: Option<String>,
    pub blockchain_id: Uuid,
    pub node_type: serde_json::Value,
    pub address: Option<String>,
    pub wallet_address: Option<String>,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub chain_status: NodeChainStatus,
    pub sync_status: NodeSyncStatus,
    pub staking_status: Option<NodeStakingStatus>,
    pub container_status: ContainerStatus,
    pub self_update: bool,
    pub vcpu_count: i64,
    pub mem_size_mb: i64,
    pub disk_size_gb: i64,
    pub network: String,
}

pub struct NodeUpdateRequest {
    pub id: Uuid,
    pub host_id: Option<String>,
    pub name: Option<String>,
    pub ip_addr: Option<String>,
    pub chain_status: Option<NodeChainStatus>,
    pub sync_status: Option<NodeSyncStatus>,
    pub staking_status: Option<NodeStakingStatus>,
    pub block_height: Option<i64>,
    pub self_update: bool,
    pub container_status: Option<ContainerStatus>,
    pub address: Option<String>,
}

impl TryFrom<GrpcNodeInfo> for NodeUpdateRequest {
    type Error = ApiError;

    fn try_from(info: GrpcNodeInfo) -> Result<Self> {
        let GrpcNodeInfo {
            id,
            host_id,
            name,
            ip,
            app_status,
            sync_status,
            staking_status,
            block_height,
            self_update,
            container_status,
            onchain_name: _, // We explicitly do not use this field
            address,
        } = info;
        let req = Self {
            id: id.as_str().parse()?,
            name,
            ip_addr: ip,
            chain_status: app_status.map(|n| n.try_into()).transpose()?,
            sync_status: sync_status.map(|n| n.try_into()).transpose()?,
            staking_status: staking_status.map(|n| n.try_into()).transpose()?,
            block_height,
            self_update: self_update.unwrap_or(false),
            container_status: container_status.map(|n| n.try_into()).transpose()?,
            address,
            host_id,
        };
        Ok(req)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = nodes)]
pub struct NodeInfo {
    pub id: uuid::Uuid,
    pub version: Option<String>,
    pub ip_addr: Option<String>,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub chain_status: Option<NodeChainStatus>,
    pub sync_status: Option<NodeSyncStatus>,
    pub staking_status: Option<NodeStakingStatus>,
    pub container_status: Option<ContainerStatus>,
    pub self_update: bool,
}

impl NodeInfo {
    pub async fn update_info(
        id: &Uuid,
        info: &NodeInfo,
        tx: &mut super::DbTrx<'_>,
    ) -> Result<Node> {
        let node = diesel::update(nodes::table).set(info).get_result(tx)?;
        Ok(node)
    }
}

/// This struct is used for updating the metrics of a node.
#[derive(Debug)]
pub struct NodeMetricsUpdate {
    id: Uuid,
    height: Option<i64>,
    age: Option<i64>,
    staking: Option<NodeStakingStatus>,
    cons: Option<bool>,
    chain: Option<NodeChainStatus>,
    sync: Option<NodeSyncStatus>,
}

impl NodeMetricsUpdate {
    /// Performs a selective update of only the columns related to metrics of the provided nodes.
    pub async fn update_metrics(updates: Vec<Self>, tx: &mut super::DbTrx<'_>) -> Result<()> {
        diesel::update(nodes::table)
            .set(updates)
            .execute(tx)
            .await?;
        Ok(())
    }
}
