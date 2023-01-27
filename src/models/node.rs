use super::{node_type::*, PgQuery};
use crate::auth::FindableById;
use crate::cookbook::get_hw_requirements;
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy::{self, NodeInfo as GrpcNodeInfo};
use crate::models::node_property_value::NodeProperties;
use crate::models::{Blockchain, Host, IpAddress, UpdateInfo};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{types::Json, FromRow, Row};
use std::string::ToString;
use strum_macros::{Display, EnumString};
use uuid::Uuid;

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
#[derive(
    Clone, Copy, Debug, Display, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, EnumString,
)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_node_chain_status", rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
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

#[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
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
    pub node_type: Json<NodeProperties>,
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
    pub blockchains: Vec<Uuid>,
}

#[axum::async_trait]
impl FindableById for Node {
    async fn find_by_id(id: Uuid, db: &mut sqlx::PgConnection) -> Result<Self> {
        sqlx::query_as("SELECT * FROM nodes where id = $1")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
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

        let node = sqlx::query_as(
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
                    sync_status,
                    ip_gateway,
                    self_update,
                    vcpu_count,
                    mem_size_mb,
                    disk_size_gb,
                    host_name,
                    network
                ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21) RETURNING *"#,
        )
        .bind(req.org_id)
        .bind(host_id)
        .bind(&req.name)
        .bind(&req.groups)
        .bind(&req.version)
        .bind(&req.ip_addr)
        .bind(req.blockchain_id)
        .bind(&req.node_type)
        .bind(&req.address)
        .bind(&req.wallet_address)
        .bind(req.block_height)
        .bind(&req.node_data)
        .bind(req.chain_status)
        .bind(req.sync_status)
        .bind(&req.ip_gateway)
        .bind(req.self_update)
        .bind(requirements.vcpu_count)
        .bind(requirements.mem_size_mb)
        .bind(requirements.disk_size_gb)
        .bind(host.name)
        .bind(&req.network)
        .fetch_one(tx)
        .await
        .map_err(|e| {
            tracing::error!("Error creating node: {}", e);
            e
        })?;

        Ok(node)
    }

    pub async fn update_info(
        id: &Uuid,
        info: &NodeInfo,
        tx: &mut super::DbTrx<'_>,
    ) -> Result<Node> {
        sqlx::query_as(
            r#"UPDATE nodes SET 
                    version = COALESCE($1, version),
                    ip_addr = COALESCE($2, ip_addr),
                    block_height = COALESCE($3, block_height),
                    node_data = COALESCE($4, node_data),
                    chain_status = COALESCE($5, chain_status),
                    sync_status = COALESCE($6, sync_status),
                    self_update = COALESCE($7, self_update)
                WHERE id = $8 RETURNING *"#,
        )
        .bind(&info.version)
        .bind(&info.ip_addr)
        .bind(info.block_height)
        .bind(&info.node_data)
        .bind(info.chain_status)
        .bind(info.sync_status)
        .bind(info.self_update)
        .bind(id)
        .fetch_one(tx)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_all_by_host(host_id: Uuid, db: &mut sqlx::PgConnection) -> Result<Vec<Self>> {
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
        db: &mut sqlx::PgConnection,
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
        db: &mut sqlx::PgConnection,
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
        db: &mut sqlx::PgConnection,
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

    pub async fn running_nodes_count(org_id: &Uuid, db: &mut sqlx::PgConnection) -> Result<i32> {
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

    pub async fn halted_nodes_count(org_id: &Uuid, db: &mut sqlx::PgConnection) -> Result<i32> {
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
                         self_update = COALESCE($7, self_update)
                WHERE id = $8
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

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeCreateRequest {
    pub org_id: Uuid,
    pub host_name: String,
    pub name: Option<String>,
    pub groups: Option<String>,
    pub version: Option<String>,
    pub ip_addr: Option<String>,
    pub ip_gateway: Option<String>,
    pub blockchain_id: Uuid,
    pub node_type: Json<NodeProperties>,
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
    pub name: Option<String>,
    pub ip_addr: Option<String>,
    pub chain_status: Option<NodeChainStatus>,
    pub sync_status: Option<NodeSyncStatus>,
    pub staking_status: Option<NodeStakingStatus>,
    pub block_height: Option<i64>,
    pub self_update: bool,
}

impl TryFrom<GrpcNodeInfo> for NodeUpdateRequest {
    type Error = ApiError;

    fn try_from(info: GrpcNodeInfo) -> Result<Self> {
        let id = info.id.as_str().parse()?;
        let req = Self {
            id,
            name: info.name,
            ip_addr: info.ip,
            chain_status: info.app_status.map(|n| n.try_into()).transpose()?,
            sync_status: info.sync_status.map(|n| n.try_into()).transpose()?,
            staking_status: info.staking_status.map(|n| n.try_into()).transpose()?,
            block_height: info.block_height,
            self_update: info.self_update.unwrap_or(false),
        };
        Ok(req)
    }
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
    pub self_update: bool,
}

/// This struct is used for updating the metrics of a node.
#[derive(Debug)]
pub struct NodeMetricsUpdate {
    id: Uuid,
    height: Option<i64>,
    block_age: Option<i64>,
    staking_status: Option<NodeStakingStatus>,
    consensus: Option<bool>,
    chain_status: Option<NodeChainStatus>,
    sync_status: Option<NodeSyncStatus>,
}

impl NodeMetricsUpdate {
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
            "UPDATE nodes SET
                block_height = row.height::BIGINT,
                block_age = row.block_age::BIGINT,
                staking_status = row.staking_status::enum_node_staking_status,
                consensus = row.consensus::BOOLEAN,
                chain_status = row.chain_status::enum_node_chain_status,
                sync_status = row.sync_status::enum_node_sync_status
            FROM (
                ",
        );

        // Now we bind a variable number of parameters
        query_builder.push_values(updates.iter(), |mut builder, update| {
            builder
                .push_bind(update.id)
                .push_bind(update.height)
                .push_bind(update.block_age)
                .push_bind(update.staking_status)
                .push_bind(update.consensus)
                .push_bind(update.chain_status)
                .push_bind(update.sync_status);
        });
        // We finish the query by specifying which bind parameters mean what. NOTE: When adding
        // bind parameters they MUST be bound in the same order as they are specified below. Not
        // doing so results in incorrectly interpreted queries.
        query_builder.push(
            "
            ) AS row(id, height, block_age, staking_status, consensus, chain_status, sync_status)
            WHERE
                nodes.id = row.id::uuid;",
        );
        let template = sqlx::query(query_builder.sql());
        let query = updates.into_iter().fold(template, Self::bind_to);
        query.execute(tx).await?;
        Ok(())
    }

    pub fn from_metrics(id: String, metric: blockjoy::NodeMetrics) -> Result<Self> {
        let id = id.parse()?;
        Ok(Self {
            id,
            height: metric.height.map(i64::try_from).transpose()?,
            block_age: metric.block_age.map(i64::try_from).transpose()?,
            staking_status: metric
                .staking_status
                .map(NodeStakingStatus::try_from)
                .transpose()?,
            consensus: metric.consensus,
            chain_status: metric
                .application_status
                .map(TryInto::try_into)
                .transpose()?,
            sync_status: metric.sync_status.map(TryInto::try_into).transpose()?,
        })
    }

    /// Binds the params in `params` to the provided query in the correct order, then returns the
    /// modified query. Since this is order-dependent, this function is private.
    fn bind_to(query: PgQuery<'_>, params: Self) -> PgQuery<'_> {
        query
            .bind(params.id)
            .bind(params.height)
            .bind(params.block_age)
            .bind(params.staking_status)
            .bind(params.consensus)
            .bind(params.chain_status)
            .bind(params.sync_status)
    }
}
