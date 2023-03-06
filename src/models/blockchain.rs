use super::schema::blockchains;
use crate::errors::Result;
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
    pub id: uuid::Uuid,                            // id -> Uuid,
    pub name: String,                              // name -> Text,
    pub description: Option<String>,               // description -> Nullable<Text>,
    pub status: BlockchainStatus,                  // status -> EnumBlockchainStatus,
    pub project_url: Option<String>,               // project_url -> Nullable<Text>,
    pub repo_url: Option<String>,                  // repo_url -> Nullable<Text>,
    pub supports_etl: bool,                        // supports_etl -> Bool,
    pub supports_node: bool,                       // supports_node -> Bool,
    pub supports_staking: bool,                    // supports_staking -> Bool,
    pub supports_broadcast: bool,                  // supports_broadcast -> Bool,
    pub version: Option<String>,                   // version -> Nullable<Text>,
    pub created_at: chrono::DateTime<chrono::Utc>, // created_at -> Timestamptz,
    pub updated_at: chrono::DateTime<chrono::Utc>, // updated_at -> Timestamptz,
    pub token: Option<String>,                     // token -> Nullable<Text>,
    supported_node_types: serde_json::Value,       // supported_node_types -> Jsonb,
}

type NotDeleted =
    dsl::Filter<blockchains::table, dsl::NotEq<blockchains::status, BlockchainStatus>>;

impl Blockchain {
    pub fn supported_node_types(&self) -> Result<Vec<super::NodeType>> {
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
