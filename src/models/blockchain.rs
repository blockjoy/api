mod node_type;
pub use node_type::BlockchainNodeType;
mod property;
pub use property::{BlockchainProperty, BlockchainPropertyUiType};
mod version;
pub use version::BlockchainVersion;

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::database::Conn;
use crate::error::QueryError;

use super::schema::blockchains;

#[derive(
    Clone,
    Copy,
    Debug,
    derive_more::Display,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    derive_more::Deref,
    derive_more::From,
    derive_more::FromStr,
    derive_more::Into,
    diesel_derive_newtype::DieselNewType,
)]
pub struct BlockchainId(uuid::Uuid);

#[derive(Clone, Debug, Queryable, Identifiable, AsChangeset)]
pub struct Blockchain {
    pub id: BlockchainId,
    pub name: String,
    pub description: Option<String>,
    pub project_url: Option<String>,
    pub repo_url: Option<String>,
    pub version: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Blockchain {
    pub async fn find_all(conn: &mut Conn<'_>) -> crate::Result<Vec<Self>> {
        let chains = blockchains::table
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await?;

        Ok(chains)
    }

    pub async fn find_by_id(id: BlockchainId, conn: &mut Conn<'_>) -> crate::Result<Self> {
        blockchains::table
            .find(id)
            .get_result(conn)
            .await
            .for_table_id("blockchains", id)
    }

    pub async fn find_by_ids(
        mut ids: Vec<BlockchainId>,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        ids.sort();
        ids.dedup();
        let chains = blockchains::table
            .filter(blockchains::id.eq_any(ids))
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await?;

        Ok(chains)
    }

    pub async fn find_by_name(blockchain: &str, conn: &mut Conn<'_>) -> crate::Result<Self> {
        blockchains::table
            .filter(super::lower(blockchains::name).eq(super::lower(blockchain)))
            .first(conn)
            .await
            .for_table_id("blockchains", blockchain)
    }

    pub async fn update(&self, c: &mut Conn<'_>) -> crate::Result<Self> {
        let mut self_to_update = self.clone();
        self_to_update.updated_at = chrono::Utc::now();
        diesel::update(blockchains::table.find(self_to_update.id))
            .set(self_to_update)
            .get_result(c)
            .await
            .map_err(Into::into)
    }
}
