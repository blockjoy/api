use crate::errors::{ApiError, Result};
use crate::models::{Host, Node};
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
        let hosts = sqlx::query(
            r##"
            SELECT
                hosts.*
            FROM
                groupable
            INNER JOIN
                hosts
            ON
                hosts.id = groupable.groupable_id
            WHERE
                groupable.group_id = $1
            ORDER BY
                lower(hosts.name) DESC"##,
        )
        .bind(&id)
        .map(Host::from)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)?;

        let nodes = sqlx::query_as::<_, Node>(
            r##"
            SELECT
                nodes.*
            FROM
                groupable
            INNER JOIN
                nodes
            ON
                nodes.id = groupable.groupable_id
            WHERE
                groupable.group_id = $1
            ORDER BY
                lower(nodes.name) DESC"##,
        )
        .bind(&id)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)?;

        Ok(GroupResponse {
            group_id: id,
            nodes: (!nodes.is_empty()).then(|| nodes),
            hosts: (!hosts.is_empty()).then(|| hosts),
        })
    }

    pub async fn create(req: &GroupCreateRequest, db: &PgPool) -> Result<Self> {
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

    pub async fn update(id: Uuid, req: &GroupUpdateRequest, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>("UPDATE groups SET name = $1 WHERE id = $2 RETURNING *")
            .bind(&req.name)
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)?;

        Self::find_by_id(id, db).await
    }

    pub async fn add_members(req: &GroupMemberRequest, db: &PgPool) -> Result<Self> {
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

    pub async fn delete_members(req: &GroupMemberRequest, db: &PgPool) -> Result<u64> {
        let mut ids = Vec::<Uuid>::new();
        if let Some(nodes) = &req.nodes {
            ids.extend(nodes);
        }
        if let Some(hosts) = &req.hosts {
            ids.extend(hosts);
        }
        if !ids.is_empty() {
            let deleted_members = sqlx::query("DELETE FROM groupable WHERE groupable_id = ANY($1)")
                .bind(ids)
                .execute(db)
                .await?;
            Ok(deleted_members.rows_affected())
        } else {
            Ok(0)
        }
    }

    pub async fn delete(id: Uuid, db: &PgPool) -> Result<u64> {
        let mut tx = db.begin().await?;
        let deleted_group = sqlx::query("DELETE FROM groups WHERE id = $1")
            .bind(id)
            .execute(&mut tx)
            .await?;
        let deleted_members = sqlx::query("DELETE FROM groupable WHERE group_id = $1")
            .bind(id)
            .execute(&mut tx)
            .await?;
        tx.commit().await?;
        Ok(deleted_group.rows_affected() + deleted_members.rows_affected())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupCreateRequest {
    pub name: String,
    pub org_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupUpdateRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMemberRequest {
    pub group_id: Uuid,
    pub nodes: Option<Vec<Uuid>>,
    pub hosts: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupResponse {
    pub group_id: Uuid,
    pub nodes: Option<Vec<Node>>,
    pub hosts: Option<Vec<Host>>,
}
