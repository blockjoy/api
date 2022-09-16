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

impl From<Blockchain> for blockjoy_ui::Blockchain {
    fn from(model: Blockchain) -> Self {
        let convert_dt = |dt: DateTime<Utc>| prost_types::Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_nanos() as i32,
        };
        Self {
            id: Some(model.id.into()),
            name: model.name,
            description: model.description,
            status: model.status as i32,
            project_url: model.project_url,
            repo_url: model.repo_url,
            supports_etl: model.supports_etl,
            supports_node: model.supports_node,
            supports_staking: model.supports_staking,
            supports_broadcast: model.supports_broadcast,
            version: model.version,
            created_at: Some(convert_dt(model.created_at)),
            updated_at: Some(convert_dt(model.updated_at)),
        }
    }
}
