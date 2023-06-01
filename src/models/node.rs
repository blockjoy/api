use super::node_type::*;
use super::schema::nodes;
use super::string_to_array;
use crate::cloudflare::CloudflareApi;
use crate::cookbook::get_hw_requirements;
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

mod property;
pub use property::NodeProperty;

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
    allow_ips: serde_json::Value,
    deny_ips: serde_json::Value,
    pub node_type: NodeType,
    pub scheduler_similarity: Option<super::SimilarNodeAffinity>,
    pub scheduler_resource: Option<super::ResourceAffinity>,
}

#[derive(Clone, Debug)]
pub struct NodeFilter {
    pub org_id: uuid::Uuid,
    pub offset: u64,
    pub limit: u64,
    pub status: Vec<NodeChainStatus>,
    pub node_types: Vec<NodeType>,
    pub blockchains: Vec<uuid::Uuid>,
    pub host_id: Option<uuid::Uuid>,
}

#[derive(Clone, Debug)]
pub struct NodeSelfUpgradeFilter {
    pub node_type: NodeType,
    pub blockchain_id: uuid::Uuid,
    // Semantic versioning.
    pub version: String,
}

impl Node {
    pub async fn find_by_id(id: uuid::Uuid, conn: &mut AsyncPgConnection) -> crate::Result<Self> {
        let node = nodes::table.find(id).get_result(conn).await?;
        Ok(node)
    }

    pub async fn find_by_ids(
        ids: impl IntoIterator<Item = uuid::Uuid>,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let mut ids: Vec<uuid::Uuid> = ids.into_iter().collect();
        ids.sort();
        ids.dedup();
        let node = nodes::table
            .filter(nodes::id.eq_any(ids))
            .get_results(conn)
            .await?;
        Ok(node)
    }

    pub async fn properties(
        &self,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<NodeProperty>> {
        NodeProperty::by_node(self, conn).await
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

        if let Some(host_id) = filter.host_id {
            query = query.filter(nodes::host_id.eq(host_id));
        }

        let nodes = query.get_results(conn).await?;
        Ok(nodes)
    }

    pub async fn update(self, conn: &mut AsyncPgConnection) -> crate::Result<Self> {
        let mut node_to_update = self.clone();
        node_to_update.updated_at = chrono::Utc::now();
        let node = diesel::update(nodes::table.find(node_to_update.id))
            .set(node_to_update)
            .get_result(conn)
            .await?;
        Ok(node)
    }

    pub async fn delete(
        node_id: uuid::Uuid,
        cf_api: &CloudflareApi,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<()> {
        let node = Node::find_by_id(node_id, conn).await?;

        diesel::delete(nodes::table.find(node_id))
            .execute(conn)
            .await?;

        if let Err(e) = cf_api.remove_node_dns(node.dns_record_id).await {
            tracing::error!("Could not remove DNS for node! {e}");
        }

        Ok(())
    }

    /// Finds the next possible host for this node to be tried on.
    pub async fn find_host(&self, conn: &mut AsyncPgConnection) -> crate::Result<super::Host> {
        let chain = super::Blockchain::find_by_id(self.blockchain_id, conn).await?;
        let requirements =
            get_hw_requirements(chain.name, self.node_type.to_string(), self.version.clone())
                .await?;

        let candidates = match self.scheduler() {
            Some(scheduler) => {
                super::Host::host_candidates(
                    requirements,
                    self.blockchain_id,
                    self.node_type,
                    self.org_id,
                    scheduler,
                    conn,
                )
                .await?
            }
            None => vec![super::Host::find_by_id(self.host_id, conn).await?],
        };

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

    pub fn scheduler(&self) -> Option<super::NodeScheduler> {
        let Some(resource) = self.scheduler_resource else { return None; };
        Some(super::NodeScheduler {
            similarity: self.scheduler_similarity,
            resource,
        })
    }

    pub fn allow_ips(&self) -> crate::Result<Vec<FilteredIpAddr>> {
        Self::filtered_ip_addrs(self.allow_ips.clone())
    }

    pub fn deny_ips(&self) -> crate::Result<Vec<FilteredIpAddr>> {
        Self::filtered_ip_addrs(self.deny_ips.clone())
    }

    pub async fn find_all_to_upgrade(
        filter: &NodeSelfUpgradeFilter,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        use super::schema::blockchains;

        let nodes = nodes::table
            .inner_join(blockchains::table.on(nodes::blockchain_id.eq(blockchains::id)))
            .filter(nodes::self_update)
            .filter(nodes::node_type.eq(filter.node_type))
            .filter(string_to_array(nodes::version, ".").lt(string_to_array(&filter.version, ".")))
            .filter(blockchains::id.eq(filter.blockchain_id))
            .distinct_on(nodes::id)
            .select(nodes::all_columns)
            .get_results(conn)
            .await
            .map_err(|e| {
                tracing::error!("Error finding nodes to upgrade: {e}");
                e
            })?;

        Ok(nodes)
    }

    fn filtered_ip_addrs(value: serde_json::Value) -> crate::Result<Vec<FilteredIpAddr>> {
        let addrs: Vec<FilteredIpAddr> = serde_json::from_value(value)?;
        Ok(addrs)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FilteredIpAddr {
    pub ip: String,
    pub description: Option<String>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = nodes)]
pub struct NewNode<'a> {
    pub id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub name: String,
    pub version: &'a str,
    pub blockchain_id: uuid::Uuid,
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
    pub allow_ips: serde_json::Value,
    pub deny_ips: serde_json::Value,
    pub node_type: NodeType,
    pub created_by: uuid::Uuid,
    /// Controls whether to run the node on hosts that contain nodes similar to this one.
    pub scheduler_similarity: Option<super::SimilarNodeAffinity>,
    /// Controls whether to run the node on hosts that are full or empty.
    pub scheduler_resource: Option<super::ResourceAffinity>,
}

impl NewNode<'_> {
    pub async fn create(
        self,
        host_id: Option<uuid::Uuid>,
        cf_api: &CloudflareApi,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Node> {
        let no_sched = || anyhow!("If there is no host_id, the scheduler is required");
        let host = match host_id {
            Some(id) => super::Host::find_by_id(id, conn).await?,
            None => {
                self.find_host(self.scheduler().ok_or_else(no_sched)?, conn)
                    .await?
            }
        };
        let ip_addr = super::IpAddress::next_for_host(host.id, conn)
            .await?
            .ip
            .ip()
            .to_string();

        let ip_gateway = host.ip_gateway.ip().to_string();

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
    pub async fn find_host(
        &self,
        scheduler: super::NodeScheduler,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<super::Host> {
        use crate::Error::NoMatchingHostError;

        let chain = super::Blockchain::find_by_id(self.blockchain_id, conn).await?;
        let requirements = get_hw_requirements(
            chain.name,
            self.node_type.to_string(),
            self.version.to_string(),
        )
        .await?;
        let candidates = super::Host::host_candidates(
            requirements,
            self.blockchain_id,
            self.node_type,
            self.org_id,
            scheduler,
            conn,
        )
        .await?;
        // Just take the first one if there is one.
        let best = candidates
            .into_iter()
            .next()
            .ok_or_else(|| NoMatchingHostError("The system is out of resources".to_string()))?;
        Ok(best)
    }

    fn scheduler(&self) -> Option<super::NodeScheduler> {
        let Some(resource) = self.scheduler_resource else { return None; };
        Some(super::NodeScheduler {
            similarity: self.scheduler_similarity,
            resource,
        })
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
    pub allow_ips: Option<serde_json::Value>,
    pub deny_ips: Option<serde_json::Value>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models;

    #[tokio::test]
    async fn can_filter_nodes() -> anyhow::Result<()> {
        let mut name = String::from("test_");
        name.push_str(&petname::petname(3, "_"));

        let db = crate::TestDb::setup().await;
        let cloudflare = crate::TestCloudflareApi::new().await;
        let cloudflare_api = cloudflare.get_cloudflare_api();
        let blockchain = db.blockchain().await;
        let user = db.user().await;
        let org = db.org().await;
        let req = NewNode {
            id: uuid::Uuid::new_v4(),
            org_id: org.id,
            blockchain_id: blockchain.id,
            chain_status: NodeChainStatus::Unknown,
            sync_status: NodeSyncStatus::Syncing,
            container_status: ContainerStatus::Installing,
            block_height: None,
            node_data: None,
            name,
            version: "3.3.0",
            staking_status: NodeStakingStatus::Staked,
            self_update: false,
            vcpu_count: 0,
            mem_size_bytes: 0,
            disk_size_bytes: 0,
            network: "some network",
            node_type: NodeType::Validator,
            created_by: user.id,
            scheduler_similarity: None,
            scheduler_resource: Some(models::ResourceAffinity::MostResources),
            allow_ips: serde_json::json!([]),
            deny_ips: serde_json::json!([]),
        };

        let mut conn = db.conn().await;
        let host = db.host().await;
        req.create(Some(host.id), &cloudflare_api, &mut conn)
            .await
            .unwrap();

        let filter = models::NodeFilter {
            status: vec![models::NodeChainStatus::Unknown],
            node_types: vec![],
            blockchains: vec![blockchain.id],
            limit: 10,
            offset: 0,
            org_id: org.id,
            host_id: Some(host.id),
        };

        let nodes = models::Node::filter(filter, &mut conn).await?;

        assert_eq!(nodes.len(), 1);

        Ok(())
    }
}
