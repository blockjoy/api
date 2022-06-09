use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
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

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Groupable {
    id: Uuid,
    group_id: Uuid,
    groupable_id: Uuid,
    groupable_type: GroupableType,
}
