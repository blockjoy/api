use super::node_type::*;
use super::schema::{nodes, orgs_users};
use crate::auth::FindableById;
use crate::cloudflare::CloudflareApi;
use crate::cookbook::get_hw_requirements;
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

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

/// NodeSyncStatus reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumNodeSyncStatus"]
pub enum NodeSyncStatus {
    Unknown,
    Syncing,
    Synced,
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

#[derive(Clone, Debug, Queryable, AsChangeset)]
pub struct Node {
    pub id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub host_id: uuid::Uuid,
    pub name: String,
    pub groups: Option<String>,
    pub version: String,
    pub ip_addr: String,
    pub address: Option<String>,
    pub wallet_address: Option<String>,
    pub block_height: Option<i64>,
    pub node_data: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub blockchain_id: uuid::Uuid,
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
    pub org_id: uuid::Uuid,
    pub offset: u64,
    pub limit: u64,
    pub status: Vec<NodeChainStatus>,
    pub node_types: Vec<NodeType>,
    pub blockchains: Vec<uuid::Uuid>,
}

#[axum::async_trait]
impl FindableById for Node {
    async fn find_by_id(id: uuid::Uuid, conn: &mut AsyncPgConnection) -> crate::Result<Self> {
        let node = nodes::table.find(id).get_result(conn).await?;
        Ok(node)
    }
}

impl Node {
    pub fn properties(&self) -> crate::Result<super::NodeProperties> {
        let res = serde_json::from_value(self.properties.clone())?;
        Ok(res)
    }

    pub async fn all(conn: &mut AsyncPgConnection) -> crate::Result<Vec<Self>> {
        nodes::table
            .get_results(conn)
            .await
            .map_err(crate::Error::from)
    }

    pub async fn find_all_by_host(
        host_id: uuid::Uuid,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let nodes = nodes::table
            .filter(nodes::host_id.eq(host_id))
            .get_results(conn)
            .await?;
        Ok(nodes)
    }

    pub async fn find_all_by_org(
        org_id: uuid::Uuid,
        offset: i64,
        limit: i64,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
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
        org_id: uuid::Uuid,
        user_id: uuid::Uuid,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<bool> {
        let query = orgs_users::table
            .filter(orgs_users::org_id.eq(org_id))
            .filter(orgs_users::user_id.eq(user_id));
        let exists = diesel::select(diesel::dsl::exists(query))
            .get_result(conn)
            .await?;
        Ok(exists)
    }

    pub async fn filter(
        filter: NodeFilter,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let mut query = nodes::table
            .filter(nodes::org_id.eq(filter.org_id))
            .offset(filter.offset.try_into()?)
            .limit(filter.limit.try_into()?)
            .into_boxed();

        // Apply filters if present
        if !filter.blockchains.is_empty() {
            query = query.filter(nodes::blockchain_id.eq_any(&filter.blockchains));
        }

        if !filter.status.is_empty() {
            query = query.filter(nodes::chain_status.eq_any(&filter.status));
        }

        if !filter.node_types.is_empty() {
            query = query.filter(nodes::node_type.eq_any(&filter.node_types));
        }

        let nodes = query.get_results(conn).await?;
        Ok(nodes)
    }

    pub async fn running_nodes_count(
        org_id: uuid::Uuid,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<i64> {
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

    pub async fn halted_nodes_count(
        org_id: uuid::Uuid,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<i64> {
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

    pub async fn update(self, conn: &mut AsyncPgConnection) -> crate::Result<Self> {
        let node = diesel::update(nodes::table.find(self.id))
            .set((self, nodes::updated_at.eq(chrono::Utc::now())))
            .get_result(conn)
            .await?;
        Ok(node)
    }

    pub async fn delete(node_id: uuid::Uuid, conn: &mut AsyncPgConnection) -> crate::Result<()> {
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

    pub async fn find_host(&self, conn: &mut AsyncPgConnection) -> crate::Result<super::Host> {
        let chain = super::Blockchain::find_by_id(self.blockchain_id, conn).await?;
        let requirements =
            get_hw_requirements(chain.name, self.node_type.to_string(), self.version.clone())
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
        let hosts_tried = super::NodeLog::hosts_tried(&deployments, conn).await?;
        let best = match (hosts_tried.as_slice(), candidates.len()) {
            // If there are 0 hosts to try, we return an error.
            (_, 0) => return Err(anyhow!("No available host candidates").into()),
            // If we are on the first host to try we just take the first candidate.
            ([], _) => candidates[0].clone(),
            // If we are on the first host to try and we tried once, we try that host again.
            ([(host, 1)], 1) => host.clone(),
            // Now we need at least two candidates, so lets check for that.
            (_, 1) => return Err(anyhow!("Only available host already failed twice").into()),
            // If there is 1 host that we tried so far, we can try a new one
            ([_], _) => candidates[1].clone(),
            // If we are on the second host to try and we tried once, we try that host again.
            ([_, (host, 1)], _) => host.clone(),
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
    pub blockchain_id: uuid::Uuid,
    pub node_type: NodeType,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = nodes)]
pub struct NewNode<'a> {
    pub id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub name: String,
    pub groups: String,
    pub version: &'a str,
    pub blockchain_id: uuid::Uuid,
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
    pub fn properties(&self) -> crate::Result<super::NodeProperties> {
        let res = serde_json::from_value(self.properties.clone())?;
        Ok(res)
    }

    pub async fn create(self, conn: &mut AsyncPgConnection) -> crate::Result<Node> {
        use crate::Error::NoMatchingHostError;

        let host = self
            .find_host(conn)
            .await
            .map_err(|_| NoMatchingHostError("The system is out of resources".to_string()))?;
        let ip_addr = super::IpAddress::next_for_host(host.id, conn)
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
        let chain = super::Blockchain::find_by_id(self.blockchain_id, conn).await?;
        let requirements = get_hw_requirements(
            chain.name,
            self.node_type.to_string(),
            self.version.to_string(),
        )
        .await?;
        let candidates = dbg!(
            super::Host::host_candidates(
                requirements,
                self.blockchain_id,
                self.node_type,
                self.org_id,
                self.scheduler(),
                conn,
            )
            .await
        )?;
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
    pub async fn update(&self, conn: &mut AsyncPgConnection) -> crate::Result<Node> {
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
    pub id: uuid::Uuid,
    pub block_height: Option<i64>,
    pub block_age: Option<i64>,
    pub staking_status: Option<NodeStakingStatus>,
    pub consensus: Option<bool>,
    pub chain_status: Option<NodeChainStatus>,
    pub sync_status: Option<NodeSyncStatus>,
}

impl UpdateNodeMetrics {
    /// Performs a selective update of only the columns related to metrics of the provided nodes.
    pub async fn update_metrics(
        updates: Vec<Self>,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<()> {
        for update in updates {
            diesel::update(nodes::table.find(update.id))
                .set(update)
                .execute(conn)
                .await?;
        }
        Ok(())
    }
}
