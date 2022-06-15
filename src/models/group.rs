use crate::errors::{ApiError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_groupable_type", rename_all = "snake_case")]
pub enum GroupableType {
    Host,
    Node,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Group {
    id: Uuid,
    name: String,
    org_id: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Group {
    pub async fn create(req: &GroupRequest, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>(
            r##"
            INSERT INTO groups
                (name, org_id)
            VALUES
                ($1,$2)
            RETURNING *
            "##,
        )
        .bind(&req.name)
        .bind(&req.org_id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Groupable {
    id: Uuid,
    group_id: Uuid,
    groupable_id: Uuid,
    groupable_type: GroupableType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRequest {
    pub name: String,
    pub org_id: Uuid,
}
