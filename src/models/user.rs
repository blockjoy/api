use crate::auth::expiration_provider::ExpirationProvider;
use crate::auth::{
    token::TokenError, FindableById, Identifiable, JwtToken, TokenClaim, TokenRole, TokenType,
    UserAuthToken, UserRefreshToken,
};
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy_ui::LoginUserRequest;
use crate::mail::MailClient;
use crate::models::{org::Org, validator::StakeStatus, FEE_BPS_DEFAULT, STAKE_QUOTA_DEFAULT};
use anyhow::anyhow;
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    #[serde(skip_serializing)]
    pub hashword: String,
    #[serde(skip_serializing)]
    pub salt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh: Option<String>,
    pub fee_bps: i64,
    pub staking_quota: i64,
    pub created_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserSelectiveUpdate {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub fee_bps: Option<i64>,
    pub staking_quota: Option<i64>,
    pub refresh_token: Option<String>,
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
    /// Test if given `token` has expired and refresh it using the `refresh_token` if necessary
    pub async fn verify_and_refresh_auth_token(
        token: UserAuthToken,
        refresh_token: UserRefreshToken,
        db: &PgPool,
    ) -> Result<(Option<User>, UserAuthToken, UserRefreshToken)> {
        if token.has_expired() && refresh_token.has_expired() {
            Err(ApiError::from(TokenError::Expired))
        } else if token.has_expired() && !refresh_token.has_expired() {
            // Generate new auth token
            let claim = TokenClaim::new(
                token.get_id(),
                ExpirationProvider::expiration(TokenType::UserAuth),
                TokenType::UserAuth,
                TokenRole::User,
                None,
            );
            let token = UserAuthToken::try_new(claim)?;
            let claim = TokenClaim::new(
                token.get_id(),
                ExpirationProvider::expiration(TokenType::UserRefresh),
                TokenType::UserRefresh,
                TokenRole::User,
                None,
            );
            let refresh_token = UserRefreshToken::try_new(claim)?;
            let fields = UserSelectiveUpdate {
                refresh_token: Some(refresh_token.encode()?),
                ..Default::default()
            };
            let user = User::update_all(refresh_token.get_id(), fields, db).await?;

            Ok((Some(user), token, refresh_token))
        } else if !token.has_expired() && refresh_token.has_expired() {
            Err(ApiError::from(TokenError::RefreshTokenError(anyhow!(
                "Refresh token expired"
            ))))
        } else {
            // Token is valid, just return what we got
            // If nothing was updated or changed, we don't even query for the user to save 1 query
            Ok((None, token, refresh_token))
        }
    }

    pub fn verify_password(&self, password: &str) -> Result<()> {
        let argon2 = Argon2::default();
        let parsed_hash = argon2.hash_password_simple(password.as_bytes(), &self.salt)?;

        if let Some(output) = parsed_hash.hash {
            if self.hashword == output.to_string() {
                return Ok(());
            }
        }

        Err(ApiError::InvalidAuthentication(anyhow!(
            "Invalid email or password."
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
            "SELECT count(*) FROM validators where user_id = $1 AND stake_status = $2 AND deleted_at IS NULL",
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
            "SELECT id, pay_address FROM users where pay_address is not NULL AND deleted_at IS NULL",
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
                users.id = $1 AND deleted_at IS NULL
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
                WHERE deleted_at IS NULL
                ORDER BY
                    users.email
            "##
        )
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_by_email(email: &str, db: &PgPool) -> Result<Self> {
        sqlx::query_as(
            r#"SELECT * FROM users WHERE LOWER(email) = LOWER($1) AND deleted_at IS NULL limit 1"#,
        )
        .bind(email)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_by_refresh(refresh: &str, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>(
            "SELECT * FROM users WHERE refresh = $1 AND deleted_at IS NULL limit 1",
        )
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
                "UPDATE users set hashword = $1, salt = $2 WHERE id = $3 AND deleted_at IS NULL RETURNING *",
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

    pub async fn create(user: UserRequest, db: &PgPool, _role: Option<TokenRole>) -> Result<Self> {
        user.validate()
            .map_err(|e| ApiError::ValidationError(e.to_string()))?;

        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        if let Some(hashword) = argon2
            .hash_password_simple(user.password.as_bytes(), salt.as_str())?
            .hash
        {
            let id = Uuid::new_v4();
            let mut tx = db.begin().await?;
            let result = match sqlx::query_as::<_, Self>(
                r#"INSERT INTO users 
                    (email, first_name, last_name, hashword, salt, staking_quota, fee_bps, id, refresh)
                    values 
                    (LOWER($1),$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *"#,
            )
            .bind(user.email)
            .bind(user.first_name)
            .bind(user.last_name)
            .bind(hashword.to_string())
            .bind(salt.as_str())
            .bind(STAKE_QUOTA_DEFAULT)
            .bind(FEE_BPS_DEFAULT)
            .bind(id)
            .bind(UserRefreshToken::create(id).encode()?)
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
                    .await?;

                    sqlx::query(
                        "INSERT INTO orgs_users (org_id, user_id, role) values($1, $2, 'owner')",
                    )
                    .bind(org.id)
                    .bind(user.id)
                    .execute(&mut tx)
                    .await?;

                    Ok(user)
                }
                Err(e) => Err(e),
            };

            tx.commit().await?;

            result
        } else {
            Err(ApiError::ValidationError("Invalid password.".to_string()))
        }
    }

    /// Check if user can be found by email, is confirmed and has provided a valid password
    pub async fn login(login: LoginUserRequest, db: &PgPool) -> Result<Self> {
        let user = Self::find_by_email(&login.email, db).await.map_err(|_e| {
            ApiError::InvalidAuthentication(anyhow!("Email or password is invalid."))
        })?;

        if User::is_confirmed(user.id, db).await? {
            match user.verify_password(&login.password) {
                Ok(_) => Ok(user),
                Err(e) => Err(e),
            }
        } else {
            Err(ApiError::UserConfirmationError)
        }
    }

    pub async fn refresh(id: Uuid, refresh_token: String, db: &PgPool) -> Result<User> {
        // Update user with new refresh token
        let fields = UserSelectiveUpdate {
            first_name: None,
            last_name: None,
            fee_bps: None,
            staking_quota: None,
            refresh_token: Some(refresh_token.clone()),
        };

        Self::update_all(id, fields, db).await
    }

    /// QR Code data for specific invoice
    pub async fn get_qr_by_id(db: &PgPool, user_id: Uuid) -> Result<String> {
        let user_summary = Self::find_summary_by_user(db, user_id).await?;

        let mut bal = user_summary.balance();
        if bal < 0 {
            bal = 0;
        }

        if let Some(pay_address) = user_summary.pay_address.as_ref() {
            let hnt = bal as f64 / 100000000.00;
            return Ok(format!(
                r#"{{"type":"payment","address":"{pay_address}","amount":{hnt:.8}}}"#,
            ));
        }

        Err(ApiError::UnexpectedError(anyhow!("No Balance")))
    }

    pub async fn update_all(id: Uuid, fields: UserSelectiveUpdate, db: &PgPool) -> Result<Self> {
        let mut tx = db.begin().await?;
        let user = sqlx::query_as::<_, User>(
            r#"UPDATE users SET 
                    first_name = COALESCE($1, first_name),
                    last_name = COALESCE($2, last_name),
                    fee_bps = COALESCE($3, fee_bps),
                    staking_quota = COALESCE($4, staking_quota),
                    refresh = COALESCE($5, refresh)
                WHERE id = $6 AND deleted_at IS NULL RETURNING *"#,
        )
        .bind(fields.first_name)
        .bind(fields.last_name)
        .bind(fields.fee_bps)
        .bind(fields.staking_quota)
        .bind(fields.refresh_token)
        .bind(id)
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await?;

        Ok(user)
    }

    pub async fn confirm(id: Uuid, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, User>(
            r#"UPDATE users SET 
                    confirmed_at = now()
                WHERE id = $1 and confirmed_at IS NULL AND deleted_at IS NULL RETURNING *"#,
        )
        .bind(id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn is_confirmed(id: Uuid, db: &PgPool) -> Result<bool> {
        let result: i32 = sqlx::query_scalar(
            r#"SELECT count(*)::int 
            FROM users WHERE id = $1 AND confirmed_at IS NOT NULL AND deleted_at IS NULL"#,
        )
        .bind(id)
        .fetch_one(db)
        .await?;

        Ok(result == 1)
    }

    /// Mark user deleted if no more nodes belong to it
    pub async fn delete(id: Uuid, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, User>(
            r#"UPDATE users u SET
                    deleted_at = now()
                WHERE id = $1
                    AND (SELECT (COUNT(*) > 0) as delete_me from nodes LEFT JOIN orgs_users ou on u.id = ou.user_id)
                RETURNING *"#,
        )
        .bind(id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
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
        sqlx::query_as::<_, Self>(
            "SELECT * FROM users WHERE id = $1 AND deleted_at IS NULL limit 1",
        )
        .bind(id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }
}

impl Identifiable for User {
    fn get_id(&self) -> Uuid {
        self.id
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
    pub first_name: String,
    pub last_name: String,
    #[validate(length(min = 8), must_match = "password_confirm")]
    pub password: String,
    pub password_confirm: String,
}

// #[derive(Debug, Clone, Serialize, Deserialize, Validate)]
// pub struct PasswordResetRequest {
//     #[validate(email)]
//     pub email: String,
// }

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserPayAddress {
    pub id: Uuid,
    pub pay_address: String,
}
