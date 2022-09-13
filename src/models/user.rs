//! TODO: @tstaetter For now I've removed all JWT token related stuff, that needs to be reimplemented
//!         using the new token respecting possible new workflows (eg magic link) TBD

use crate::auth::{FindableById, TokenHolderType, TokenIdentifyable, TokenType};
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy_ui::LoginUserRequest;
use crate::mail::MailClient;
use crate::models::{
    org::Org,
    token::{Token, TokenRole, UserToken},
    validator::StakeStatus,
    FEE_BPS_DEFAULT, STAKE_QUOTA_DEFAULT,
};
use anyhow::anyhow;
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(skip_serializing)]
    pub hashword: String,
    #[serde(skip_serializing)]
    pub salt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh: Option<String>,
    pub fee_bps: i64,
    pub staking_quota: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserSelectiveUpdate {
    pub email: Option<String>,
    pub fee_bps: Option<i64>,
    pub staking_quota: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserLogin {
    pub(crate) id: Uuid,
    pub(crate) email: String,
    pub(crate) fee_bps: i64,
    pub(crate) staking_quota: i64,
    pub(crate) token: String,
}

impl User {
    pub fn verify_password(&self, password: &str) -> Result<()> {
        let argon2 = Argon2::default();
        let parsed_hash = argon2.hash_password_simple(password.as_bytes(), &self.salt)?;

        if let Some(output) = parsed_hash.hash {
            if self.hashword == output.to_string() {
                return Ok(());
            }
        }

        Err(ApiError::InvalidAuthentication(anyhow!(
            "Inavlid email or password."
        )))
    }

    // pub async fn reset_password(_db: &PgPool, _req: &PwdResetInfo) -> Result<User> {
    //     // TODO: use new auth
    //     unimplemented!()
    // }

    pub async fn email_reset_password(&self, db: &PgPool) -> Result<()> {
        let client = MailClient::new();
        client.reset_password(self, db).await
    }

    pub async fn can_stake(&self, db: &PgPool, count: i64) -> Result<bool> {
        Ok(self.staking_quota >= (self.staking_count(db).await? + count))
    }

    /// Returns the number of validators in "Staking"
    pub async fn staking_count(&self, db: &PgPool) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM validators where user_id = $1 AND stake_status = $2",
        )
        .bind(self.id)
        .bind(StakeStatus::Staking)
        .fetch_one(db)
        .await?;

        Ok(row.0)
    }

    pub async fn find_all(db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query_as::<_, Self>("SELECT * FROM users")
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_all_pay_address(db: &PgPool) -> Result<Vec<UserPayAddress>> {
        sqlx::query_as::<_, UserPayAddress>(
            "SELECT id, pay_address FROM users where pay_address is not NULL",
        )
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_summary_by_user(db: &PgPool, user_id: Uuid) -> Result<UserSummary> {
        Ok(sqlx::query_as::<_, UserSummary>(r##"
            SELECT 
                users.id, 
                email,
                pay_address,
                staking_quota,
                fee_bps,
                (SELECT count(*) from validators where validators.user_id=users.id)::BIGINT as validator_count,
                COALESCE((SELECT sum(rewards.amount) from rewards where rewards.user_id=users.id), 0)::BIGINT as rewards_total,
                COALESCE((SELECT sum(invoices.amount) FROM invoices where invoices.user_id = users.id), 0)::BIGINT as invoices_total,
                COALESCE((SELECT sum(payments.amount) FROM payments where payments.user_id = users.id), 0)::BIGINT as payments_total,
                users.created_at as joined_at
            FROM
                users
            WHERE
                users.id = $1
        "##)
        .bind(user_id)
        .fetch_one(db)
        .await?)
    }

    /// Gets a summary list of all users
    pub async fn find_all_summary(db: &PgPool) -> Result<Vec<UserSummary>> {
        sqlx::query_as::<_, UserSummary>(
            r##"
                SELECT 
                    users.id, 
                    email,
                    pay_address,
                    staking_quota,
                    fee_bps,
                    (SELECT count(*) from validators where validators.user_id=users.id)::BIGINT as validator_count,
                    COALESCE((SELECT sum(rewards.amount) from rewards where rewards.user_id=users.id), 0)::BIGINT as rewards_total,
                    COALESCE((SELECT sum(invoices.amount) FROM invoices where invoices.user_id = users.id), 0)::BIGINT as invoices_total,
                    COALESCE((SELECT sum(payments.amount) FROM payments where payments.user_id = users.id), 0)::BIGINT as payments_total,
                    users.created_at as joined_at
                FROM
                    users
                ORDER BY
                    users.email
            "##
        )
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_by_email(email: &str, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>(
            r#"SELECT u.*, t.token, t.role FROM users u
                        RIGHT JOIN tokens t on u.id = t.user_id
                    WHERE LOWER(email) = LOWER($1)"#,
        )
        .bind(email)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
        /*
        sqlx::query_as::<_, Self>("SELECT * FROM users WHERE LOWER(email) = LOWER($1) limit 1")
            .bind(email)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
         */
    }

    pub async fn find_by_refresh(refresh: &str, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>("SELECT * FROM users WHERE refresh = $1 limit 1")
            .bind(refresh)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn update_password(&self, password: &str, db: &PgPool) -> Result<Self> {
        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        if let Some(hashword) = argon2
            .hash_password_simple(password.as_bytes(), salt.as_str())?
            .hash
        {
            return sqlx::query_as::<_, Self>(
                "UPDATE users set hashword = $1, salt = $2 WHERE id = $3 RETURNING *",
            )
            .bind(hashword.to_string())
            .bind(salt.as_str())
            .bind(self.id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from);
        }

        Err(ApiError::ValidationError("Invalid password.".to_string()))
    }

    pub async fn create(user: UserRequest, db: &PgPool, role: Option<TokenRole>) -> Result<Self> {
        user.validate()
            .map_err(|e| ApiError::ValidationError(e.to_string()))?;

        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        if let Some(hashword) = argon2
            .hash_password_simple(user.password.as_bytes(), salt.as_str())?
            .hash
        {
            let mut tx = db.begin().await?;
            let result = match sqlx::query_as::<_, Self>(
                "INSERT INTO users (email, hashword, salt, staking_quota, fee_bps) values (LOWER($1),$2,$3,$4,$5) RETURNING *",
            )
            .bind(user.email)
            .bind(hashword.to_string())
            .bind(salt.as_str())
            .bind(STAKE_QUOTA_DEFAULT)
            .bind(FEE_BPS_DEFAULT)
            .fetch_one(&mut tx)
            .await
            .map_err(ApiError::from)
            {
                Ok(user) => {
                    let org = sqlx::query_as::<_, Org>(
                        "INSERT INTO orgs (name, is_personal) values (LOWER($1), true) RETURNING *",
                    )
                        .bind(&user.email)
                        .fetch_one(&mut tx)
                        .await
                        .map_err(ApiError::from)?;

                    sqlx::query(
                        "INSERT INTO orgs_users (org_id, user_id, role) values($1, $2, 'owner')",
                    )
                        .bind(org.id)
                        .bind(user.id)
                        .execute(&mut tx)
                        .await
                        .map_err(ApiError::from)?;

                    Ok(user)
                },
                Err(e) => Err(e),
            };

            tx.commit().await?;

            let user = result?;
            let role = role.unwrap_or(TokenRole::User);
            Token::create_for::<User>(&user, role, TokenType::Login, db).await?;
            Ok(user)
        } else {
            Err(ApiError::ValidationError("Invalid password.".to_string()))
        }
    }

    pub async fn login(login: LoginUserRequest, db: &PgPool) -> Result<Self> {
        let user = Self::find_by_email(&login.email, db).await.map_err(|_e| {
            ApiError::InvalidAuthentication(anyhow!("Email or password is invalid."))
        })?;

        match user.verify_password(&login.password) {
            Ok(_) => Ok(user),
            Err(e) => Err(e),
        }
    }

    pub async fn refresh(req: UserRefreshRequest, db: &PgPool) -> Result<User> {
        Self::find_by_refresh(&req.refresh, db).await
    }

    /// QR Code data for specific invoice
    pub async fn get_qr_by_id(db: &PgPool, user_id: Uuid) -> Result<String> {
        let user_summary = Self::find_summary_by_user(db, user_id).await?;

        let mut bal = user_summary.balance();
        if bal < 0 {
            bal = 0;
        }

        if user_summary.pay_address.is_some() {
            let hnt = bal as f64 / 100000000.00;
            return Ok(format!(
                r#"{{"type":"payment","address":"{}","amount":{:.8}}}"#,
                user_summary.pay_address.as_ref().unwrap(),
                hnt
            ));
        }

        Err(ApiError::UnexpectedError(anyhow!("No Balance")))
    }

    pub async fn update_all(id: Uuid, fields: UserSelectiveUpdate, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await.unwrap();
        let user = sqlx::query_as::<_, User>(
            r#"UPDATE users SET 
                    email = COALESCE($1, email),
                    fee_bps = COALESCE($2, fee_bps),
                    staking_quota = COALESCE($3, staking_quota),
                WHERE id = $5 RETURNING *"#,
        )
        .bind(fields.email)
        .bind(fields.fee_bps)
        .bind(fields.staking_quota)
        .bind(id)
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await.unwrap();

        Ok(user)
    }

    pub fn preferred_language(&self) -> &str {
        // Needs to be done later, but we want to have some stub in place so we keep our code aware
        // of language differences.
        "en"
    }
}

#[axum::async_trait]
impl FindableById for User {
    async fn find_by_id(id: Uuid, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>("SELECT * FROM users WHERE id = $1 limit 1")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }
}

#[axum::async_trait]
impl TokenIdentifyable for User {
    async fn set_token(token_id: Uuid, user_id: Uuid, db: &PgPool) -> Result<()> {
        let user_token = UserToken::new(user_id, token_id, TokenType::Login);
        user_token.create_or_update(db).await?;
        Ok(())
    }

    fn get_holder_type() -> TokenHolderType {
        TokenHolderType::User
    }

    fn get_id(&self) -> Uuid {
        self.id
    }

    async fn delete_token(user_id: Uuid, db: &PgPool) -> Result<()> {
        UserToken::delete_by_user(user_id, TokenType::Login, db).await?;
        Ok(())
    }

    async fn get_token(&self, db: &PgPool) -> Result<Token> {
        Token::get::<User>(self.id, TokenType::Login, db).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserSummary {
    pub id: Uuid,
    pub email: String,
    pub pay_address: Option<String>,
    pub staking_quota: i64,
    pub fee_bps: i64,
    pub validator_count: i64,
    pub rewards_total: i64,
    pub invoices_total: i64,
    pub payments_total: i64,
    pub joined_at: DateTime<Utc>,
}

impl UserSummary {
    pub fn balance(&self) -> i64 {
        self.invoices_total - self.payments_total
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UserRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8), must_match = "password_confirm")]
    pub password: String,
    pub password_confirm: String,
}

// #[derive(Debug, Clone, Serialize, Deserialize, Validate)]
// pub struct PasswordResetRequest {
//     #[validate(email)]
//     pub email: String,
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRefreshRequest {
    pub refresh: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserPayAddress {
    pub id: Uuid,
    pub pay_address: String,
}
