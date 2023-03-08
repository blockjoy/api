use super::schema::broadcast_filters;
use crate::errors::{ApiError, Result};
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Queryable)]
pub struct BroadcastFilter {
    pub id: Uuid,
    pub blockchain_id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub callback_url: String,
    pub auth_token: String,
    pub is_active: bool,
    pub last_processed_height: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub addresses: Option<serde_json::Value>,
    pub txn_types: serde_json::Value,
}

impl BroadcastFilter {
    pub fn addresses(&self) -> Result<Option<Vec<String>>> {
        let addresses = self
            .addresses
            .clone()
            .map(serde_json::from_value)
            .transpose()?;
        Ok(addresses)
    }

    pub fn txn_types(&self) -> Result<Vec<String>> {
        let txn_types = serde_json::from_value(self.txn_types.clone())?;
        Ok(txn_types)
    }

    pub async fn find_by_id(id: Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let filter = broadcast_filters::table.find(id).get_result(conn).await?;
        Ok(filter)
    }

    pub async fn find_all_by_org_id(
        org_id: Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<Self>> {
        let filter = broadcast_filters::table
            .filter(broadcast_filters::org_id.eq(org_id))
            .get_results(conn)
            .await?;
        Ok(filter)
    }

    pub async fn delete(id: Uuid, conn: &mut AsyncPgConnection) -> Result<()> {
        diesel::delete(broadcast_filters::table.find(id))
            .execute(conn)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Validate, Insertable)]
#[diesel(table_name = broadcast_filters)]
pub struct CreateBroadcastFilter {
    pub blockchain_id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub addresses: serde_json::Value,
    pub callback_url: String,
    pub auth_token: String,
    pub txn_types: serde_json::Value,
    pub is_active: bool,
}

impl CreateBroadcastFilter {
    pub async fn create(self, conn: &mut AsyncPgConnection) -> Result<BroadcastFilter> {
        self.validate()
            .map_err(|e| ApiError::ValidationError(e.to_string()))?;
        let filter = diesel::insert_into(broadcast_filters::table)
            .values(self)
            .get_result(conn)
            .await?;
        Ok(filter)
    }
}

#[derive(Debug, Clone, Validate, AsChangeset)]
#[diesel(table_name = broadcast_filters)]
pub struct UpdateBroadcastFilter {
    pub id: Uuid,
    pub blockchain_id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub addresses: serde_json::Value,
    pub callback_url: String,
    pub auth_token: String,
    pub txn_types: serde_json::Value,
    pub is_active: bool,
}

impl UpdateBroadcastFilter {
    pub async fn update(self, conn: &mut AsyncPgConnection) -> Result<BroadcastFilter> {
        self.validate()
            .map_err(|e| ApiError::ValidationError(e.to_string()))?;
        let filter = diesel::update(broadcast_filters::table.find(self.id))
            .set((self, broadcast_filters::updated_at.eq(chrono::Utc::now())))
            .get_result(conn)
            .await?;
        Ok(filter)
    }
}

#[derive(Debug, Clone)]
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
