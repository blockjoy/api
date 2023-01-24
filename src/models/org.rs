use crate::auth::{FindableById, Identifiable};
use crate::errors::{ApiError, Result};
use crate::models::User;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_org_role", rename_all = "snake_case")]
pub enum OrgRole {
    Admin,
    Owner,
    Member,
}

impl Display for OrgRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OrgRole::Admin => write!(f, "admin"),
            OrgRole::Owner => write!(f, "owner"),
            OrgRole::Member => write!(f, "member"),
        }
    }
}

impl Identifiable for OrgUser {
    fn get_id(&self) -> Uuid {
        self.user_id
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct Org {
    pub id: Uuid,
    pub name: String,
    pub is_personal: bool,
    #[sqlx(default)]
    pub role: Option<OrgRole>,
    #[sqlx(default)]
    pub member_count: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[tonic::async_trait]
impl FindableById for Org {
    async fn find_by_id(id: Uuid, db: &mut sqlx::PgConnection) -> Result<Self> {
        sqlx::query_as("SELECT *, (SELECT count(*) from orgs_users where orgs_users.org_id = orgs.id) as member_count FROM orgs where id = $1 and deleted_at IS NULL")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }
}

impl Org {
    pub async fn find_all(db: &mut sqlx::PgConnection) -> Result<Vec<Org>> {
        sqlx::query_as("SELECT * FROM orgs ORDER BY id LIMIT 1")
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_user(
        org_id: Uuid,
        user_id: Uuid,
        db: &mut sqlx::PgConnection,
    ) -> Result<Org> {
        sqlx::query_as::<_, Self>(
            r##"
            SELECT 
                orgs.*,
                orgs_users.role,
                (SELECT count(*) from orgs_users where orgs_users.org_id = orgs.id) as member_count
            FROM 
                orgs 
            INNER JOIN 
                orgs_users 
            ON 
                orgs.id = orgs_users.org_id 
            WHERE 
                orgs_users.user_id = $1 AND orgs.id = $2 and orgs.deleted_at IS NULL
            "##,
        )
        .bind(user_id)
        .bind(org_id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_all_by_user(user_id: Uuid, db: &mut sqlx::PgConnection) -> Result<Vec<Org>> {
        sqlx::query_as::<_, Self>(
            r##"
            SELECT
                orgs.*,
                orgs_users.role,
                (SELECT count(*) from orgs_users where orgs_users.org_id = orgs.id) as member_count
            FROM
                orgs
            INNER JOIN
                orgs_users
            ON
                orgs.id = orgs_users.org_id
            WHERE
                orgs_users.user_id = $1 and orgs.deleted_at IS NULL
            ORDER BY
                orgs.is_personal desc,
                lower(orgs.name)
            "##,
        )
        .bind(user_id)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    /// Returns the users of an organization
    pub async fn find_all_members(
        org_id: Uuid,
        db: &mut sqlx::PgConnection,
    ) -> Result<Vec<OrgUser>> {
        sqlx::query_as("SELECT * FROM orgs_users WHERE org_id = $1")
            .bind(org_id)
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    /// Returns the users of an organization
    pub async fn find_all_member_users(
        org_id: Uuid,
        db: &mut sqlx::PgConnection,
    ) -> Result<Vec<User>> {
        sqlx::query_as::<_, User>(
            r#"SELECT users.*
                        FROM users
                            RIGHT JOIN orgs_users ou
                                ON users.id = ou.user_id
                        WHERE ou.org_id = $1"#,
        )
        .bind(org_id)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_personal_org(user_id: Uuid, db: &mut sqlx::PgConnection) -> Result<Org> {
        sqlx::query_as::<_, Self>(
            r##"
            SELECT
                orgs.*,
                orgs_users.role,
                (SELECT count(*) from orgs_users where orgs_users.org_id = orgs.id) as member_count
            FROM
                orgs
            INNER JOIN
                orgs_users
            ON
                orgs.id = orgs_users.org_id
            WHERE
                orgs_users.user_id = $1 and is_personal and orgs_users.role = 'owner'
            LIMIT 1
            "##,
        )
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    /// Returns the users of an organization
    pub async fn find_all_member_users_paginated(
        org_id: Uuid,
        limit: i32,
        offset: i32,
        db: &mut sqlx::PgConnection,
    ) -> Result<Vec<User>> {
        sqlx::query_as::<_, User>(
            r#"SELECT users.*
                        FROM users
                            RIGHT JOIN orgs_users ou
                                ON users.id = ou.user_id
                        WHERE ou.org_id = $1
                        ORDER BY users.email
                        LIMIT $2 OFFSET $3"#,
        )
        .bind(org_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    /// Checks if the user is a member
    pub async fn is_member(
        user_id: Uuid,
        org_id: Uuid,
        db: &mut sqlx::PgConnection,
    ) -> Result<bool> {
        match Self::find_org_user(user_id, org_id, db).await {
            Ok(_) => Ok(true),
            Err(ApiError::NotFoundError(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn add_member(
        user_id: Uuid,
        org_id: Uuid,
        role: OrgRole,
        tx: &mut super::DbTrx<'_>,
    ) -> Result<OrgUser> {
        sqlx::query_as(
            "INSERT INTO orgs_users (org_id, user_id, role) values ($1, $2, $3) RETURNING *",
        )
        .bind(org_id)
        .bind(user_id)
        .bind(role)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::from)
    }

    /// Returns the user role in the organization
    pub async fn find_org_user(
        user_id: Uuid,
        org_id: Uuid,
        db: &mut sqlx::PgConnection,
    ) -> Result<OrgUser> {
        let org_user =
            sqlx::query_as("SELECT * FROM orgs_users WHERE org_id = $1 AND user_id = $2")
                .bind(org_id)
                .bind(user_id)
                .fetch_one(db)
                .await?;

        Ok(org_user)
    }

    pub async fn remove_org_user(
        user_id: Uuid,
        org_id: Uuid,
        tx: &mut super::DbTrx<'_>,
    ) -> Result<OrgUser> {
        sqlx::query_as("DELETE FROM orgs_users WHERE org_id = $1 AND user_id = $2 RETURNING *")
            .bind(org_id)
            .bind(user_id)
            .fetch_one(tx)
            .await
            .map_err(ApiError::from)
    }

    /// Creates a new organization
    pub async fn create(req: &OrgRequest, user_id: Uuid, tx: &mut super::DbTrx<'_>) -> Result<Org> {
        let org_id = Uuid::new_v4();
        let org = sqlx::query_as(
            "INSERT INTO orgs (id,name,is_personal) values ($1,$2,false) RETURNING *",
        )
        .bind(org_id)
        .bind(&req.name)
        .fetch_one(&mut *tx)
        .await?;

        let _org_user = sqlx::query(
            "INSERT INTO orgs_users (org_id, user_id, role) values($1, $2, 'owner') RETURNING *",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_one(tx)
        .await?;

        Ok(org)
    }

    /// Updates an organization
    pub async fn update(id: Uuid, req: OrgRequest, tx: &mut super::DbTrx<'_>) -> Result<Self> {
        let org = sqlx::query_as(
            "UPDATE orgs SET name = $1 WHERE id = $2 and orgs.deleted_at IS NULL RETURNING *",
        )
        .bind(&req.name)
        .bind(id)
        .fetch_one(tx)
        .await?;
        Ok(org)
    }

    /// Marks the the given organization as deleted
    pub async fn delete(id: Uuid, tx: &mut super::DbTrx<'_>) -> Result<Self> {
        sqlx::query_as::<_, Org>(
            r#"
            UPDATE orgs SET deleted_at = now() 
            WHERE id = $1 AND is_personal = false
            returning *
            "#,
        )
        .bind(id)
        .fetch_one(tx)
        .await
        .map_err(ApiError::from)
    }

    /// Unmarks the the given organization as deleted
    pub async fn restore(id: Uuid, tx: &mut super::DbTrx<'_>) -> Result<Self> {
        sqlx::query_as::<_, Org>(
            r#"
            UPDATE orgs SET deleted_at = NULL 
            WHERE id = $1 AND is_personal = false
            returning *"#,
        )
        .bind(id)
        .fetch_one(tx)
        .await
        .map_err(ApiError::from)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrgRequest {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct OrgUser {
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub role: OrgRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
