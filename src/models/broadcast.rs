use crate::errors::{ApiError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::{FromRow, PgPool};
use std::result::Result as StdResult;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BroadcastFilter {
    pub id: Uuid,
    pub blockchain_id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    #[serde(
        serialize_with = "serialize_as_array",
        deserialize_with = "deserialize_as_array"
    )]
    pub addresses: Option<String>,
    pub callback_url: String,
    pub auth_token: String,
    pub txn_types: String,
    pub is_active: bool,
    pub last_processed_height: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Custom serializer implementation for casting `Option<String>` to `Option<Vec<&str>>`
fn serialize_as_array<S>(addresses: &Option<String>, s: S) -> StdResult<S::Ok, S::Error>
where
    S: Serializer,
{
    match addresses {
        Some(addresses) => {
            let addresses: Vec<&str> = addresses
                .split(", ")
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .collect();
            s.serialize_some(&addresses)
        }
        None => s.serialize_none(),
    }
}

/// Custom deserializer implementation for casting `Option<Vec<String>>` to `Option<String>`
fn deserialize_as_array<'de, D>(deserializer: D) -> StdResult<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let addresses: Option<Vec<String>> = Deserialize::deserialize(deserializer)?;
    Ok(addresses.map(|v| v.join(", ")))
}

impl BroadcastFilter {
    pub async fn find_by_id(id: &Uuid, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>("SELECT * FROM broadcast_filters where id = $1")
            .bind(&id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_all_by_org_id(org_id: &Uuid, db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query_as::<_, Self>("SELECT * FROM broadcast_filters where org_id = $1")
            .bind(&org_id)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn create(req: &BroadcastFilterRequest, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>(
            r##"
            INSERT INTO broadcast_filters
                (blockchain_id, org_id, name, addresses, callback_url, auth_token, txn_types, is_active)
            VALUES
                ($1,$2,$3,$4,$5,$6,$7,$8)
            RETURNING *
            "##)
        .bind(&req.blockchain_id)
        .bind(&req.org_id)
        .bind(&req.name)
        .bind(&req.addresses.join(", "))
        .bind(&req.callback_url)
        .bind(&req.auth_token)
        .bind(&req.txn_types)
        .bind(&req.is_active)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn update(id: &Uuid, req: &BroadcastFilterRequest, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>(
            r##"
            UPDATE broadcast_filters
                SET blockchain_id=$1, org_id=$2, name=$3, addresses=$4, callback_url=$5, auth_token=$6, txn_types=$7, is_active=$8
            WHERE
                id=$9
            RETURNING *
            "##)
        .bind(&req.blockchain_id)
        .bind(&req.org_id)
        .bind(&req.name)
        .bind(&req.addresses.join(", "))
        .bind(&req.callback_url)
        .bind(&req.auth_token)
        .bind(&req.txn_types)
        .bind(&req.is_active)
        .bind(&id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn delete(id: &Uuid, db: &PgPool) -> Result<()> {
        let _ = sqlx::query("DELETE FROM broadcast_filters WHERE id = $1")
            .bind(&id)
            .execute(db)
            .await?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastFilterRequest {
    pub blockchain_id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub addresses: Vec<String>,
    pub callback_url: String,
    pub auth_token: String,
    pub txn_types: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BroadcastLog {
    pub id: Uuid,
    pub blockchain_id: Uuid,
    pub org_id: Uuid,
    pub broadcast_filter_id: Uuid,
    pub address_count: i64,
    pub txn_count: i64,
    pub event_type: String,
    pub event_msg: Option<String>,
    pub created_at: DateTime<Utc>,
}
