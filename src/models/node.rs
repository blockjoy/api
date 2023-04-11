use super::node_type::*;
use super::schema::{nodes, orgs_users};
use crate::auth::FindableById;
use crate::cloudflare::CloudflareApi;
use crate::cookbook::get_hw_requirements;
use crate::models::{Blockchain, Host, IpAddress};
use crate::{Error, Result};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use uuid::Uuid;

/// ContainerStatus reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumContainerStatus"]
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
    type Error = Error;

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
            _ => Err(Error::UnexpectedError(anyhow!(
                "Cannot convert {n} to ContainerStatus"
            ))),
        }
    }
}

/// NodeSyncStatus reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumNodeSyncStatus"]
pub enum NodeSyncStatus {
    Unknown,
    Syncing,
    Synced,
}

impl TryFrom<i32> for NodeSyncStatus {
    type Error = Error;

    fn try_from(n: i32) -> Result<Self> {
        match n {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Syncing),
            2 => Ok(Self::Synced),
            _ => Err(Error::UnexpectedError(anyhow!(
                "Cannot convert {n} to NodeSyncStatus"
            ))),
        }
    }
}

/// NodeStakingStatus reflects blockjoy.api.v1.node.NodeInfo.StakingStatus in node.proto
#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumNodeStakingStatus"]
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
    type Error = Error;

    fn try_from(n: i32) -> Result<Self> {
        match n {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Follower),
            2 => Ok(Self::Staked),
            3 => Ok(Self::Staking),
            4 => Ok(Self::Validating),
            5 => Ok(Self::Consensus),
            6 => Ok(Self::Unstaked),
            _ => Err(Error::UnexpectedError(anyhow!(
                "Cannot convert {n} to NodeStakingStatus"
            ))),
        }
    }
}

/// NodeChainStatus reflects blockjoy.api.v1.node.NodeInfo.ApplicationStatus in node.proto
#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumNodeChainStatus"]
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
    type Error = Error;

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
            _ => Err(Error::UnexpectedError(anyhow!(
                "Cannot convert {n} to NodeChainStatus"
            ))),
        }
    }
}

impl std::str::FromStr for NodeChainStatus {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "unknown" => Ok(Self::Unknown),
            "provisioning" => Ok(Self::Provisioning),
            "broadcasting" => Ok(Self::Broadcasting),
            "cancelled" => Ok(Self::Cancelled),
            "delegating" => Ok(Self::Delegating),
            "delinquent" => Ok(Self::Delinquent),
            "disabled" => Ok(Self::Disabled),
            "earning" => Ok(Self::Earning),
            "electing" => Ok(Self::Electing),
            "elected" => Ok(Self::Elected),
            "exported" => Ok(Self::Exported),
            "ingesting" => Ok(Self::Ingesting),
            "mining" => Ok(Self::Mining),
            "minting" => Ok(Self::Minting),
            "processing" => Ok(Self::Processing),
            "relaying" => Ok(Self::Relaying),
            "removed" => Ok(Self::Removed),
            "removing" => Ok(Self::Removing),
            _ => Err(Error::UnexpectedError(anyhow!(
                "Cannot convert `{s}` to NodeChainStatus"
            ))),
        }
    }
}

#[derive(Clone, Debug, Queryable, AsChangeset)]
pub struct Node {
    pub id: Uuid,
    pub org_id: Uuid,
    pub host_id: Uuid,
    pub name: String,
    pub groups: Option<String>,
    pub version: Option<String>,
    pub ip_addr: String,
    pub address: Option<String>,
    pub wallet_address: Option<String>,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub blockchain_id: Uuid,
    pub sync_status: NodeSyncStatus,
    pub chain_status: NodeChainStatus,
    pub staking_status: Option<NodeStakingStatus>,
    pub container_status: ContainerStatus,
    properties: serde_json::Value,
    pub ip_gateway: String,
    pub self_update: bool,
    pub block_age: Option<i64>,
    pub consensus: Option<bool>,
    pub vcpu_count: i64,
    pub mem_size_bytes: i64,
    pub disk_size_bytes: i64,
    pub host_name: String,
    pub network: String,
    pub created_by: Option<uuid::Uuid>,
    pub dns_record_id: String,
    pub allow_ips: serde_json::Value,
    pub deny_ips: serde_json::Value,
    pub node_type: NodeType,
    pub scheduler_similarity: Option<super::SimilarNodeAffinity>,
    pub scheduler_resource: super::ResourceAffinity,
}

#[derive(Clone, Debug)]
pub struct NodeFilter {
    pub status: Vec<NodeChainStatus>,
    pub node_types: Vec<i32>,
    pub blockchains: Vec<uuid::Uuid>,
}

#[axum::async_trait]
impl FindableById for Node {
    async fn find_by_id(id: uuid::Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let node = nodes::table.find(id).get_result(conn).await?;
        Ok(node)
    }
}

impl Node {
    pub fn properties(&self) -> Result<super::NodeProperties> {
        let res = serde_json::from_value(self.properties.clone())?;
        Ok(res)
    }

    pub async fn all(conn: &mut AsyncPgConnection) -> Result<Vec<Self>> {
        nodes::table
            .get_results(conn)
            .await
            .map_err(crate::Error::from)
    }

    pub async fn find_all_by_host(
        host_id: Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<Self>> {
        let nodes = nodes::table
            .filter(nodes::host_id.eq(host_id))
            .get_results(conn)
            .await?;
        Ok(nodes)
    }

    pub async fn find_all_by_org(
        org_id: Uuid,
        offset: i64,
        limit: i64,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<Self>> {
        let nodes = nodes::table
            .filter(nodes::org_id.eq(org_id))
            .offset(offset)
            .limit(limit)
            .get_results(conn)
            .await?;
        Ok(nodes)
    }

    // TODO: Check role if user is allowed to delete the node
    pub async fn belongs_to_user_org(
        org_id: Uuid,
        user_id: Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<bool> {
        let query = orgs_users::table
            .filter(orgs_users::org_id.eq(org_id))
            .filter(orgs_users::user_id.eq(user_id));
        let exists = diesel::select(diesel::dsl::exists(query))
            .get_result(conn)
            .await?;
        Ok(exists)
    }

    pub async fn find_all_by_filter(
        org_id: Uuid,
        filter: NodeFilter,
        offset: i64,
        limit: i64,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<Self>> {
        let mut query = nodes::table
            .filter(nodes::org_id.eq(org_id))
            .offset(offset)
            .limit(limit)
            .into_boxed();

        // Apply filters if present
        if !filter.blockchains.is_empty() {
            tracing::debug!("Applying blockchain filter: {:?}", filter.blockchains);

            // If a list of blockchains was given, we interpret them as ids, and require that each
            // node's blockchain is in the provided list.
            query = query.filter(nodes::blockchain_id.eq_any(&filter.blockchains));
        }

        if !filter.status.is_empty() {
            query = query.filter(nodes::chain_status.eq_any(&filter.status));
        }

        let mut nodes: Vec<Node> = query.get_results(conn).await?;

        if !filter.node_types.is_empty() {
            nodes.retain(|n| filter.node_types.contains(&n.node_type.into()));
        }
        Ok(nodes)
    }

    pub async fn running_nodes_count(org_id: Uuid, conn: &mut AsyncPgConnection) -> Result<i64> {
        use NodeChainStatus::*;
        const RUNNING_STATUSES: [NodeChainStatus; 14] = [
            Broadcasting,
            Provisioning,
            Cancelled,
            Delegating,
            Delinquent,
            Earning,
            Electing,
            Elected,
            Exported,
            Ingesting,
            Mining,
            Minting,
            Processing,
            Relaying,
        ];
        let count = nodes::table
            .filter(nodes::org_id.eq(org_id))
            .filter(nodes::chain_status.eq_any(&RUNNING_STATUSES))
            .count()
            .get_result(conn)
            .await?;

        Ok(count)
    }

    pub async fn halted_nodes_count(org_id: &Uuid, conn: &mut AsyncPgConnection) -> Result<i64> {
        use NodeChainStatus::*;
        const HALTED_STATUSES: [NodeChainStatus; 4] = [Unknown, Disabled, Removed, Removing];
        let count = nodes::table
            .filter(nodes::org_id.eq(org_id))
            .filter(nodes::chain_status.eq_any(&HALTED_STATUSES))
            .count()
            .get_result(conn)
            .await?;

        Ok(count)
    }

    pub async fn update(self, conn: &mut AsyncPgConnection) -> Result<Self> {
        let node = diesel::update(nodes::table.find(self.id))
            .set((self, nodes::updated_at.eq(chrono::Utc::now())))
            .get_result(conn)
            .await?;
        Ok(node)
    }

    pub async fn delete(node_id: Uuid, conn: &mut AsyncPgConnection) -> Result<()> {
        let node = Node::find_by_id(node_id, conn).await?;
        let cf_api = CloudflareApi::new(node.ip_addr)?;

        diesel::delete(nodes::table.find(node_id))
            .execute(conn)
            .await?;

        if let Err(e) = cf_api.remove_node_dns(node.dns_record_id).await {
            tracing::error!("Could not remove DNS for node! {e}");
        }

        Ok(())
    }

    pub async fn find_host(&self, conn: &mut AsyncPgConnection) -> Result<Host> {
        let chain = Blockchain::find_by_id(self.blockchain_id, conn).await?;
        let requirements = get_hw_requirements(
            chain.name,
            self.node_type.to_string(),
            self.version.as_deref(),
        )
        .await?;

        let candidates = super::Host::host_candidates(
            requirements,
            self.blockchain_id,
            self.node_type,
            self.org_id,
            self.scheduler(),
            conn,
        )
        .await?;

        // We now have a list of host candidates for our nodes. Now the only thing left to do is to
        // make a decision about where to place the node.
        let deployments = super::NodeLog::by_node(self, conn).await?;
        let n_hosts_tried = super::NodeLog::n_hosts_tried(&deployments);
        let n_last_host = super::NodeLog::n_deploys_tried_on_last_host(&deployments);
        let best = match (n_hosts_tried, n_last_host) {
            // If we on the first host we tried, and we tried zero or one time so far, we try
            // (again) on the first host.
            (0, 0 | 1) => candidates[0].clone(),
            // Otherwise if we are on the first host, we move on to the second host.
            (0, _) => candidates[1].clone(),
            // If we are on the second host we tried, and we tried zero or one time here, we try
            // (again) on the second host.
            (1, 1) => candidates[1].clone(),
            // Otherwise we exhausted our our options and return an error.
            (_, _) => return Err(anyhow!("No available hosts").into()),
        };

        Ok(best)
    }

    fn scheduler(&self) -> super::NodeScheduler {
        super::NodeScheduler {
            similarity: self.scheduler_similarity,
            resource: self.scheduler_resource,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NodeProvision {
    pub blockchain_id: Uuid,
    pub node_type: NodeType,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = nodes)]
pub struct NewNode<'a> {
    pub id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub name: String,
    pub groups: String,
    pub version: Option<&'a str>,
    pub blockchain_id: Uuid,
    pub properties: serde_json::Value,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub chain_status: NodeChainStatus,
    pub sync_status: NodeSyncStatus,
    pub staking_status: NodeStakingStatus,
    pub container_status: ContainerStatus,
    pub self_update: bool,
    pub vcpu_count: i64,
    pub mem_size_bytes: i64,
    pub disk_size_bytes: i64,
    pub network: &'a str,
    pub node_type: NodeType,
    pub created_by: uuid::Uuid,
    pub scheduler_similarity: Option<super::SimilarNodeAffinity>,
    pub scheduler_resource: super::ResourceAffinity,
}

impl NewNode<'_> {
    pub fn properties(&self) -> Result<super::NodeProperties> {
        let res = serde_json::from_value(self.properties.clone())?;
        Ok(res)
    }

    pub async fn create(self, conn: &mut AsyncPgConnection) -> Result<Node> {
        use crate::Error::NoMatchingHostError;

        let host = self
            .find_host(conn)
            .await
            .map_err(|_| NoMatchingHostError("The system is out of resources".to_string()))?;
        let ip_addr = IpAddress::next_for_host(host.id, conn)
            .await?
            .ip
            .ip()
            .to_string();

        let ip_gateway = host.ip_gateway.ip().to_string();

        let cf_api = CloudflareApi::new(ip_addr.clone())?;
        let dns_record_id = cf_api
            .get_node_dns(self.name.clone(), ip_addr.clone())
            .await?;

        diesel::insert_into(nodes::table)
            .values((
                self,
                nodes::host_id.eq(host.id),
                nodes::ip_gateway.eq(ip_gateway),
                nodes::ip_addr.eq(ip_addr),
                nodes::host_name.eq(&host.name),
                nodes::dns_record_id.eq(dns_record_id),
            ))
            .get_result(conn)
            .await
            .map_err(|e| {
                tracing::error!("Error creating node: {e}");
                e.into()
            })
    }

    /// Finds the most suitable host to initially place the node on. Since this is a freshly created
    /// node, we do not need to worry about logic regarding where the retry placing the node. We
    /// simply ask for an ordered list of the most suitable hosts, and pick the first one.
    pub async fn find_host(&self, conn: &mut AsyncPgConnection) -> crate::Result<super::Host> {
        let chain = Blockchain::find_by_id(self.blockchain_id, conn).await?;
        let requirements =
            get_hw_requirements(chain.name, self.node_type.to_string(), self.version).await?;
        let candidates = super::Host::host_candidates(
            requirements,
            self.blockchain_id,
            self.node_type,
            self.org_id,
            self.scheduler(),
            conn,
        )
        .await?;
        // Jus take the first one if ther is one.
        let best = candidates
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No matching host found"))?;
        Ok(best)
    }

    fn scheduler(&self) -> super::NodeScheduler {
        super::NodeScheduler {
            similarity: self.scheduler_similarity,
            resource: self.scheduler_resource,
        }
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = nodes)]
pub struct UpdateNode<'a> {
    pub id: uuid::Uuid,
    pub name: Option<&'a str>,
    pub version: Option<&'a str>,
    pub ip_addr: Option<&'a str>,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub chain_status: Option<NodeChainStatus>,
    pub sync_status: Option<NodeSyncStatus>,
    pub staking_status: Option<NodeStakingStatus>,
    pub container_status: Option<ContainerStatus>,
    pub self_update: Option<bool>,
    pub address: Option<&'a str>,
}

impl UpdateNode<'_> {
    pub async fn update(&self, conn: &mut AsyncPgConnection) -> Result<Node> {
        let node = diesel::update(nodes::table.find(self.id))
            .set((self, nodes::updated_at.eq(chrono::Utc::now())))
            .get_result(conn)
            .await?;
        Ok(node)
    }
}

/// This struct is used for updating the metrics of a node.
#[derive(Debug, Insertable, AsChangeset)]
#[diesel(table_name = nodes)]
pub struct UpdateNodeMetrics {
    pub id: Uuid,
    pub block_height: Option<i64>,
    pub block_age: Option<i64>,
    pub staking_status: Option<NodeStakingStatus>,
    pub consensus: Option<bool>,
    pub chain_status: Option<NodeChainStatus>,
    pub sync_status: Option<NodeSyncStatus>,
}

impl UpdateNodeMetrics {
    /// Performs a selective update of only the columns related to metrics of the provided nodes.
    pub async fn update_metrics(updates: Vec<Self>, conn: &mut AsyncPgConnection) -> Result<()> {
        for update in updates {
            diesel::update(nodes::table.find(update.id))
                .set(update)
                .execute(conn)
                .await?;
        }
        Ok(())
    }
}
