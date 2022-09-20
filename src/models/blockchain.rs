use crate::{
    errors::{ApiError, Result},
    grpc::blockjoy_ui,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_blockchain_status", rename_all = "snake_case")]
pub enum BlockchainStatus {
    Development,
    Alpha,
    Beta,
    Production,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Blockchain {
    pub id: Uuid,
    pub name: String,
    pub token: Option<String>,
    pub description: Option<String>,
    pub status: BlockchainStatus,
    pub project_url: Option<String>,
    pub repo_url: Option<String>,
    pub supports_etl: bool,
    pub supports_node: bool,
    pub supports_staking: bool,
    pub supports_broadcast: bool,
    pub version: Option<String>,
    pub supported_node_types: Vec<super::NodeType>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Blockchain {
    pub async fn find_all(db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query_as("SELECT * FROM blockchains WHERE status <> 'deleted' order by lower(name)")
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_id(id: uuid::Uuid, db: &PgPool) -> Result<Self> {
        sqlx::query_as("SELECT * FROM blockchains WHERE status <> 'deleted' AND id = $1")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }
}
