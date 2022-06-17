use crate::errors::{ApiError, Result};
use crate::models::Host;
use crate::models::Node;
use anyhow::anyhow;
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
pub struct Groupable {
    id: Uuid,
    group_id: Uuid,
    groupable_id: Uuid,
    groupable_type: GroupableType,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub org_id: Uuid,
    #[sqlx(default)]
    pub member_count: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Group {
    pub async fn find_by_id(id: Uuid, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>(
            r##"
            SELECT
                groups.*,
                (SELECT count(*) from groupable where groupable.group_id = groups.id) as member_count
            FROM
                groups
            WHERE
                groups.id = $1"##
            )
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn get_members(id: Uuid, db: &PgPool) -> Result<GroupResponse> {
        let items = sqlx::query_as::<_, Groupable>(
            r##"
            SELECT
                *
            FROM
                groupable
            WHERE
                group_id = $1"##,
        )
        .bind(id)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)?;

        let mut nodes = Vec::new();
        let mut hosts = Vec::new();

        for item in items {
            match item.groupable_type {
                GroupableType::Node => {
                    nodes.push(item.groupable_id);
                }
                GroupableType::Host => {
                    hosts.push(item.groupable_id);
                }
            }
        }
        Ok(GroupResponse {
            group_id: id,
            nodes: (!nodes.is_empty()).then(|| nodes),
            hosts: (!hosts.is_empty()).then(|| hosts),
        })
    }

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

    pub async fn add(req: &GroupAddRequest, db: &PgPool) -> Result<Self> {
        let mut group = Self::find_by_id(req.group_id, db).await?;

        let (items, type_) = if let Some(nodes) = &req.nodes {
            Ok((
                Node::find_all_by_ids(nodes, db)
                    .await?
                    .iter()
                    .map(|v| v.id)
                    .collect::<Vec<Uuid>>(),
                GroupableType::Node,
            ))
        } else if let Some(hosts) = &req.hosts {
            Ok((
                Host::find_all_by_ids(hosts, db)
                    .await?
                    .iter()
                    .map(|v| v.id)
                    .collect::<Vec<Uuid>>(),
                GroupableType::Host,
            ))
        } else {
            Err(ApiError::UnexpectedError(anyhow!("missing group items")))
        }?;

        for item in items {
            sqlx::query(
                "INSERT INTO groupable (group_id, groupable_id, groupable_type) values($1, $2, $3)",
            )
            .bind(group.id)
            .bind(item)
            .bind(type_)
            .execute(db)
            .await
            .map_err(ApiError::from)?;
            if let Some(member_count) = group.member_count.as_mut() {
                *member_count += 1;
            }
        }
        Ok(group)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRequest {
    pub name: String,
    pub org_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAddRequest {
    pub group_id: Uuid,
    pub nodes: Option<Vec<Uuid>>,
    pub hosts: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupResponse {
    pub group_id: Uuid,
    pub nodes: Option<Vec<Uuid>>,
    pub hosts: Option<Vec<Uuid>>,
}
