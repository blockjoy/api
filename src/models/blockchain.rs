use super::schema::blockchains;
use crate::Result;
use diesel::{dsl, prelude::*};
use diesel_async::{AsyncPgConnection, RunQueryDsl};

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumBlockchainStatus"]
pub enum BlockchainStatus {
    Development,
    Alpha,
    Beta,
    Production,
    Deleted,
}

#[derive(Clone, Debug, Queryable, Identifiable)]
pub struct Blockchain {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub status: BlockchainStatus,
    pub project_url: Option<String>,
    pub repo_url: Option<String>,
    pub supports_etl: bool,
    pub supports_node: bool,
    pub supports_staking: bool,
    pub supports_broadcast: bool,
    pub version: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub token: Option<String>,
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

    fn not_deleted() -> NotDeleted {
        blockchains::table.filter(blockchains::status.ne(BlockchainStatus::Deleted))
    }
}
