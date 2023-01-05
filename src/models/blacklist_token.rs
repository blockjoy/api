use crate::auth::TokenType;
use crate::errors::{ApiError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BlacklistToken {
    pub token: String,
    pub token_type: TokenType,
}

impl BlacklistToken {
    pub async fn create(token: String, token_type: TokenType, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>(
            r#"
                INSERT INTO token_blacklist 
                    (token, token_type)
                values 
                    ($1, $2)
                RETURNING *
                "#,
        )
        .bind(token)
        .bind(token_type)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    /// Returns true if token is on the blacklist
    pub async fn is_listed(token: String, db: &PgPool) -> Result<bool> {
        let res: i32 =
            sqlx::query_scalar("SELECT count(token)::int from token_blacklist WHERE token = $1")
                .bind(token)
                .fetch_one(db)
                .await?;

        Ok(res > 0)
    }
}
