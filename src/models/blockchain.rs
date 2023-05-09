use super::schema::blockchains;
use super::BlockchainPropertyValue;
use crate::Result;
use diesel::{dsl, prelude::*};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use tracing::log::warn;

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
    supported_node_types: serde_json::Value,
}

type NotDeleted =
    dsl::Filter<blockchains::table, dsl::NotEq<blockchains::status, BlockchainStatus>>;

impl Blockchain {
    pub fn supported_node_types(&self) -> Result<Vec<super::BlockchainProperties>> {
        let res = serde_json::from_value(self.supported_node_types.clone())?;
        Ok(res)
    }

    pub async fn find_all(conn: &mut AsyncPgConnection) -> Result<Vec<Self>> {
        let chains = Self::not_deleted()
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await?;

        Ok(chains)
    }

    pub async fn find_by_id(id: uuid::Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let chain = Self::not_deleted().find(id).get_result(conn).await?;

        Ok(chain)
    }

    pub async fn find_by_ids(
        ids: &[uuid::Uuid],
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<Self>> {
        let chains = Self::not_deleted()
            .filter(blockchains::id.eq_any(ids))
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await?;

        Ok(chains)
    }

    pub async fn find_by_name(blockchain: &str, c: &mut AsyncPgConnection) -> Result<Self> {
        blockchains::table
            .filter(super::lower(blockchains::name).eq(super::lower(blockchain)))
            .first(c)
            .await
            .map_err(Into::into)
    }

    pub async fn update(&self, c: &mut AsyncPgConnection) -> Result<Self> {
        let mut self_to_update = self.clone();
        self_to_update.updated_at = chrono::Utc::now();
        diesel::update(blockchains::table.find(self_to_update.id))
            .set(self_to_update)
            .get_result(c)
            .await
            .map_err(Into::into)
    }

    pub fn set_new_supported_node_type_version(
        &mut self,
        filter: &super::NodeSelfUpgradeFilter,
    ) -> Result<()> {
        let supported_node_types = &self.supported_node_types()?;
        if supported_node_types
            .iter()
            .any(|x| x.version == filter.version)
        {
            warn!(
                "Node type version {} already exists in blockchain {}",
                filter.blockchain, filter.version
            );
            return Ok(());
        }
        let previous_node_type = supported_node_types
            .iter()
            .find(|x| x.id == filter.node_type as i32);
        let properties: Option<Vec<BlockchainPropertyValue>>;
        if let Some(previous_node_type) = previous_node_type {
            properties = previous_node_type.properties.clone();
        } else {
            properties = Some(vec![BlockchainPropertyValue {
                name: "self-hosted".to_string(),
                default: None,
                ui_type: super::BlockchainPropertyUiType::Text,
                disabled: false,
                required: false,
            }]);
        }
        let new_supported_type = super::BlockchainProperties {
            id: filter.node_type as i32,
            version: filter.version.clone(),
            properties,
        };
        supported_node_types.to_owned().push(new_supported_type);
        self.supported_node_types = serde_json::to_value(supported_node_types)?;
        Ok(())
    }

    fn not_deleted() -> NotDeleted {
        blockchains::table.filter(blockchains::status.ne(BlockchainStatus::Deleted))
    }
}
