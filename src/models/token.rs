use crate::auth::{AuthToken, JwtToken, TokenHolderType, TokenIdentifyable, TokenType};
use crate::errors::ApiError;
use crate::errors::Result;
use crate::models::{host::Host, node::Node, user::User, validator::Validator};
use anyhow::anyhow;
use base64::encode as base64_encode;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::env;
use std::ops::Add;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_token_role", rename_all = "snake_case")]
pub enum TokenRole {
    Admin,
    Guest,
    Service,
    User,
    PwdReset,
}

impl ToString for TokenRole {
    fn to_string(&self) -> String {
        match self {
            TokenRole::Admin => "admin".into(),
            TokenRole::Guest => "guest".into(),
            TokenRole::Service => "service".into(),
            TokenRole::User => "user".into(),
            TokenRole::PwdReset => "pwd_reset".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Token {
    pub id: Uuid,
    pub token: String,
    pub host_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub role: TokenRole,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub type_: TokenType,
}

impl ToString for Token {
    fn to_string(&self) -> String {
        self.token.clone()
    }
}

impl Token {
    pub fn to_base64(&self) -> String {
        base64_encode(&self.token)
    }

    pub async fn get<T: TokenIdentifyable>(
        resource_id: Uuid,
        token_type: TokenType,
        db: &PgPool,
    ) -> Result<Self> {
        let id_field = T::get_holder_type().id_field();
        sqlx::query_as::<_, Self>(&format!(
            "select * from tokens where {} = $1 and type = $2",
            id_field
        ))
        .bind(resource_id)
        .bind(token_type)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn delete(id: Uuid, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await?;

        let token = sqlx::query_as::<_, Self>("DELETE FROM tokens WHERE id = $1 RETURNING *")
            .bind(id)
            .fetch_one(&mut tx)
            .await
            .map_err(ApiError::from)?;

        match tx.commit().await {
            Ok(_) => Ok(token),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn refresh(token_str: String, db: &PgPool) -> Result<Self> {
        // 1. Get the old token
        let old_token = sqlx::query_as::<_, Self>("SELECT * FROM tokens WHERE token = $1")
            .bind(token_str)
            .fetch_one(db)
            .await?;

        match old_token.host_id {
            Some(host_id) => {
                Host::delete_token(host_id, db).await?;

                return Self::delete_and_refresh::<Host>(
                    db,
                    &old_token,
                    host_id,
                    TokenHolderType::Host,
                    old_token.type_,
                )
                .await;
            }
            None => tracing::debug!("old token has no host ID"),
        }

        match old_token.user_id {
            Some(user_id) => {
                User::delete_token(user_id, db).await?;

                return Self::delete_and_refresh::<User>(
                    db,
                    &old_token,
                    user_id,
                    TokenHolderType::User,
                    old_token.type_,
                )
                .await;
            }
            None => tracing::debug!("old token has no user ID"),
        }

        Err(ApiError::UnexpectedError(anyhow!(
            "Neither host nor user ID set on token, can't refresh"
        )))
    }

    async fn delete_and_refresh<T: TokenIdentifyable>(
        db: &PgPool,
        old_token: &Token,
        resource_id: Uuid,
        holder_type: TokenHolderType,
        token_type: TokenType,
    ) -> Result<Self> {
        Token::delete(old_token.id, db).await?;
        let new_token =
            Token::create(resource_id, old_token.role, db, holder_type, token_type).await?;

        match T::set_token(new_token.id, resource_id, db).await {
            Ok(_user) => Ok(new_token),
            Err(e) => Err(e),
        }
    }

    pub async fn create_for<T: TokenIdentifyable>(
        resource: &T,
        role: TokenRole,
        token_type: TokenType,
        db: &PgPool,
    ) -> Result<Self> {
        let token = Token::create(
            resource.get_id(),
            role,
            db,
            T::get_holder_type(),
            token_type,
        )
        .await?;

        T::set_token(token.id, resource.get_id(), db).await?;
        Ok(token)
    }

    /// TODO: refactor me
    pub async fn create(
        resource_id: Uuid,
        role: TokenRole,
        db: &PgPool,
        holder_type: TokenHolderType,
        token_type: TokenType,
    ) -> Result<Self> {
        let expiration = Self::get_expiration(Self::get_expiration_period(holder_type, token_type));
        let jwt_token = AuthToken::new(resource_id, expiration.timestamp(), holder_type);
        let token_str = jwt_token.encode().unwrap();
        let id_field = match holder_type {
            TokenHolderType::User => "user_id",
            TokenHolderType::Host => "host_id",
        };

        let query = format!(
            "INSERT INTO tokens (token, {id_field}, expires_at, role, type)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *",
        );
        let token: Self = sqlx::query_as(&query)
            .bind(token_str)
            .bind(resource_id)
            .bind(expiration)
            .bind(role)
            .bind(token_type)
            .fetch_one(db)
            .await?;

        match holder_type {
            TokenHolderType::User => User::set_token(token.id, resource_id, db).await?,
            TokenHolderType::Host => Host::set_token(token.id, resource_id, db).await?,
        }
        Ok(token)
    }

    pub async fn find_by_token(token_str: &str, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>("SELECT * FROM tokens where token = $1;")
            .bind(token_str)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn get_user_for_token(
        token_str: String,
        token_type: TokenType,
        db: &PgPool,
    ) -> Result<User> {
        sqlx::query_as::<_, User>(
            "select u.* from tokens t right join users u on t.user_id = u.id where t.token = $1 and t.type = $2",
        )
        .bind(token_str)
        .bind(token_type)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn get_host_for_token(
        token_str: &str,
        token_type: TokenType,
        db: &PgPool,
    ) -> Result<Host> {
        let mut host = sqlx::query(
            "select h.* from tokens t right join hosts h on t.host_id = h.id where t.token = $1 and t.type = $2",
        )
        .bind(token_str)
        .bind(token_type)
        .map(Host::from)
        .fetch_one(db)
        .await?;

        // Add Validators list
        host.validators = Some(Validator::find_all_by_host(host.id, db).await?);
        host.nodes = Some(Node::find_all_by_host(host.id, db).await?);

        Ok(host)
    }

    fn get_expiration(duration_days: i64) -> DateTime<Utc> {
        let start = Utc::now();

        start.add(Duration::days(duration_days))
    }

    fn get_expiration_period(holder_type: TokenHolderType, token_type: TokenType) -> i64 {
        use {TokenHolderType::*, TokenType::*};

        let name = match (holder_type, token_type) {
            (User, Login) => "TOKEN_EXPIRATION_DAYS_USER",
            (User, Refresh) => "REFRESH_TOKEN_EXPIRATION_DAYS_USER",
            (User, PwdReset) => "PWD_RESET_TOKEN_EXPIRATION_DAYS_USER",
            (Host, Login) => "TOKEN_EXPIRATION_DAYS_HOST",
            (Host, Refresh) => "REFRESH_EXPIRATION_DAYS_HOST",
            (Host, PwdReset) => panic!("Host machines cannot request a password reset mail"),
        };

        env::var(name)
            .unwrap_or_else(|_| "1".into())
            .parse()
            .unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserToken {
    user_id: uuid::Uuid,
    token_id: uuid::Uuid,
    token_type: TokenType,
}

impl UserToken {
    /// Creates a new `UserToken` in-memory, but does _not_ insert it into the database.
    pub fn new(user_id: uuid::Uuid, token_id: uuid::Uuid, token_type: TokenType) -> Self {
        Self {
            user_id,
            token_id,
            token_type,
        }
    }

    /// Tries to create a new UserToken in the database, and if that fails, updates the conflicting
    /// row.
    pub async fn create_or_update(self, db: &PgPool) -> Result<Self> {
        sqlx::query_as(
            "
            INSERT INTO
                user_tokens (user_id, token_id, token_type)
            VALUES
                ($1, $2, $3)
            ON CONFLICT (user_id, token_type)
                DO UPDATE
            SET
                token_id = $2
            RETURNING
                user_id, token_id, token_type;",
        )
        .bind(self.user_id)
        .bind(self.token_id)
        .bind(self.token_type)
        .fetch_one(db)
        .await
        .map_err(Into::into)
    }

    pub async fn delete(self, db: &PgPool) -> Result<()> {
        let q = "DELETE FROM user_tokens WHERE user_id = $1 AND token_id = $2 AND token_type = $3";
        sqlx::query(q)
            .bind(self.user_id)
            .bind(self.token_id)
            .bind(self.token_type)
            .execute(db)
            .await?;
        Ok(())
    }

    pub async fn delete_by_user(
        user_id: uuid::Uuid,
        token_type: TokenType,
        db: &PgPool,
    ) -> Result<()> {
        sqlx::query("DELETE FROM user_tokens WHERE user_id = $1 AND token_type = $2;")
            .bind(user_id)
            .bind(token_type)
            .execute(db)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct HostToken {
    host_id: uuid::Uuid,
    token_id: uuid::Uuid,
    token_type: TokenType,
}

impl HostToken {
    /// Creates a new `HostToken` in-memory, but does _not_ insert it into the database.
    pub fn new(host_id: uuid::Uuid, token_id: uuid::Uuid, token_type: TokenType) -> Self {
        Self {
            host_id,
            token_id,
            token_type,
        }
    }

    /// Tries to create a new HostToken in the database, and if that fails, updates the conflicting
    /// row.
    pub async fn create_or_update(self, db: &PgPool) -> Result<Self> {
        sqlx::query_as(
            "
            INSERT INTO
                host_tokens (host_id, token_id, token_type)
            VALUES
                ($1, $2, $3)
            ON CONFLICT (host_id, token_type)
                DO UPDATE
            SET
                token_id = $2
            RETURNING
                host_id, token_id, token_type;",
        )
        .bind(self.host_id)
        .bind(self.token_id)
        .bind(self.token_type)
        .fetch_one(db)
        .await
        .map_err(Into::into)
    }

    pub async fn delete(self, db: &PgPool) -> Result<()> {
        let q = "DELETE FROM host_tokens WHERE host_id = $1 AND token_id = $2 AND token_type = $3";
        sqlx::query(q)
            .bind(self.host_id)
            .bind(self.token_id)
            .bind(self.token_type)
            .execute(db)
            .await?;
        Ok(())
    }

    pub async fn delete_by_host(
        host_id: uuid::Uuid,
        token_type: TokenType,
        db: &PgPool,
    ) -> Result<()> {
        sqlx::query("DELETE FROM host_tokens WHERE host_id = $1 AND token_type = $2;")
            .bind(host_id)
            .bind(token_type)
            .execute(db)
            .await?;
        Ok(())
    }
}
