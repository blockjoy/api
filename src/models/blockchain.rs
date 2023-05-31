use super::schema::blockchains;
use diesel::{dsl, prelude::*};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use tracing::log::warn;

mod property;
pub use property::{BlockchainProperty, BlockchainPropertyUiType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumBlockchainStatus"]
pub enum BlockchainStatus {
    Development,
    Alpha,
    Beta,
    Production,
    Deleted,
}

#[derive(Clone, Debug, Queryable, Identifiable, AsChangeset)]
pub struct Blockchain {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub status: BlockchainStatus,
    pub project_url: Option<String>,
    pub repo_url: Option<String>,
    pub version: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

type NotDeleted =
    dsl::Filter<blockchains::table, dsl::NotEq<blockchains::status, BlockchainStatus>>;

impl Blockchain {
    pub async fn find_all(conn: &mut AsyncPgConnection) -> crate::Result<Vec<Self>> {
        let chains = Self::not_deleted()
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await?;

        Ok(chains)
    }

    pub async fn find_by_id(id: uuid::Uuid, conn: &mut AsyncPgConnection) -> crate::Result<Self> {
        let chain = Self::not_deleted().find(id).get_result(conn).await?;

        Ok(chain)
    }

    pub async fn find_by_ids(
        ids: &[uuid::Uuid],
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let chains = Self::not_deleted()
            .filter(blockchains::id.eq_any(ids))
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await?;

        Ok(chains)
    }

    pub async fn find_by_name(blockchain: &str, c: &mut AsyncPgConnection) -> crate::Result<Self> {
        blockchains::table
            .filter(super::lower(blockchains::name).eq(super::lower(blockchain)))
            .first(c)
            .await
            .map_err(Into::into)
    }

    pub async fn properties(
        &self,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<BlockchainProperty>> {
        BlockchainProperty::by_blockchain(self, conn).await
    }

    pub async fn update(&self, c: &mut AsyncPgConnection) -> crate::Result<Self> {
        let mut self_to_update = self.clone();
        self_to_update.updated_at = chrono::Utc::now();
        diesel::update(blockchains::table.find(self_to_update.id))
            .set(self_to_update)
            .get_result(c)
            .await
            .map_err(Into::into)
    }

    /// Adds a new supported blockchain version for the provided (blockchain, node_type) combination
    /// by copying the required blockchain properties from an older version.
    pub async fn add_version(
        &self,
        filter: &super::NodeSelfUpgradeFilter,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<()> {
        // First we query all the props to see if the version already exists.
        let props =
            BlockchainProperty::by_blockchain_node_type(self, filter.node_type, conn).await?;
        if props.iter().any(|x| x.version == filter.version) {
            let (blockchain_id, version) = (filter.blockchain_id, &filter.version);
            warn!("Node type version {version} already exists in blockchain {blockchain_id}");
            return Ok(());
        }
        // If the version doesn't exist yet, we copy the properties from the latest version.
        let latest = props
            .iter()
            .map(|p| p.version.as_str())
            .max_by(|v1, v2| super::semver_cmp(v1, v2).unwrap_or(std::cmp::Ordering::Equal))
            .map(str::to_string);
        let to_add = if let Some(latest) = latest {
            // If there is a latest version, we can copy its props.
            props
                .into_iter()
                .filter(|prop| prop.version == latest)
                .map(|prop| BlockchainProperty {
                    id: uuid::Uuid::new_v4(),
                    version: filter.version.clone(),
                    ..prop
                })
                .collect()
        } else {
            // If no latest version exists, we default to a fallback property.
            vec![BlockchainProperty {
                id: uuid::Uuid::new_v4(),
                blockchain_id: filter.blockchain_id,
                version: filter.version.clone(),
                node_type: filter.node_type,
                name: "self-hosted".to_string(),
                default: None,
                ui_type: super::BlockchainPropertyUiType::Text,
                disabled: false,
                required: false,
            }]
        };
        BlockchainProperty::bulk_create(to_add, conn).await?;
        Ok(())
    }

    fn not_deleted() -> NotDeleted {
        blockchains::table.filter(blockchains::status.ne(BlockchainStatus::Deleted))
    }
}

#[cfg(test)]
mod tests {
    use crate::models::{NodeSelfUpgradeFilter, NodeType};

    #[tokio::test]
    async fn test_add_version_existing_version() {
        let db = crate::TestDb::setup().await;
        let mut conn = db.conn().await;
        let node_type = NodeType::Validator;
        let blockchain = db.blockchain().await;
        let n_properties = blockchain.properties(&mut conn).await.unwrap().len();
        let filter = NodeSelfUpgradeFilter {
            blockchain_id: blockchain.id,
            node_type,
            version: "3.3.0".to_string(),
        };
        blockchain.add_version(&filter, &mut conn).await.unwrap();
        let n_properties_new_final = blockchain.properties(&mut conn).await.unwrap().len();
        assert_eq!(n_properties, n_properties_new_final);
    }

    #[tokio::test]
    async fn test_add_version_non_existing_version() {
        let db = crate::TestDb::setup().await;
        let mut conn = db.conn().await;
        let node_type = NodeType::Validator;
        let blockchain = db.blockchain().await;
        let n_properties = blockchain.properties(&mut conn).await.unwrap().len();
        let filter = NodeSelfUpgradeFilter {
            blockchain_id: blockchain.id,
            node_type,
            version: "1.0.0".to_string(),
        };
        blockchain.add_version(&filter, &mut conn).await.unwrap();
        let n_properties_new_final = blockchain.properties(&mut conn).await.unwrap().len();
        assert_eq!(n_properties + 1, n_properties_new_final);
    }
}
