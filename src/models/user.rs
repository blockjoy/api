use super::{Host, Org, StakeStatus, FEE_BPS_DEFAULT, STAKE_QUOTA_DEFAULT};
use crate::auth;
use crate::errors::{ApiError, Result};
use anyhow::anyhow;
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use rand_core::OsRng;
use sendgrid::v3::*;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Row};
use std::fmt;
use std::str::FromStr;
use sqlx::postgres::PgRow;
use uuid::Uuid;
use validator::Validate;
use crate::auth::FindableByToken;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_user_role", rename_all = "snake_case")]
pub enum UserRole {
    User,
    Host,
    Admin,
}

impl fmt::Display for UserRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admin => write!(f, "admin"),
            Self::Host => write!(f, "host"),
            Self::User => write!(f, "user"),
        }
    }
}

impl FromStr for UserRole {
    type Err = ApiError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "admin" => Ok(Self::Admin),
            "host" => Ok(Self::Host),
            _ => Ok(Self::User),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserAuthInfo {
    pub id: Uuid,
    pub role: UserRole,
}

pub type AuthToken = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Authentication {
    User(UserAuthInfo),
    Host(AuthToken),
    Service(AuthToken),
}

impl Authentication {
    pub fn is_user(&self) -> bool {
        matches!(self, Self::User(_))
    }

    pub fn is_host(&self) -> bool {
        matches!(self, Self::Host(_))
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, Self::User(u) if u.role == UserRole::Admin)
    }

    pub fn is_service(&self) -> bool {
        matches!(self, Self::Service(_))
    }

    /// Returns an error if not an admin
    pub fn try_admin(&self) -> Result<bool> {
        if self.is_admin() {
            Ok(true)
        } else {
            Err(ApiError::InsufficientPermissionsError)
        }
    }

    /// Returns an error if not an host
    pub fn try_host(&self) -> Result<bool> {
        if self.is_host() {
            Ok(true)
        } else {
            Err(ApiError::InsufficientPermissionsError)
        }
    }

    /// Returns an error if not an admin
    pub fn try_service(&self) -> Result<bool> {
        if self.is_service() {
            Ok(true)
        } else {
            Err(ApiError::InsufficientPermissionsError)
        }
    }

    /// Returns an error if user doesn't have access
    pub fn try_user_access(&self, user_id: Uuid) -> Result<bool> {
        match self {
            Self::User(u) if u.id == user_id => Ok(true),
            _ => Err(ApiError::InsufficientPermissionsError),
        }
    }

    /// Returns an error if user doesn't have access
    pub async fn try_host_access(&self, host_id: Uuid, db: &PgPool) -> Result<bool> {
        if self.is_host() {
            let host = self.get_host(db).await?;
            if host.id == host_id {
                return Ok(true);
            }
        }

        Err(ApiError::InsufficientPermissionsError)
    }

    /// Returns an error if user doesn't have access to Org
    pub async fn try_org_access(&self, org_id: &Uuid, db: &PgPool) -> Result<bool> {
        match self {
            Self::User(auth) if Org::is_member(&auth.id, org_id, db).await? => Ok(true),
            _ => Err(ApiError::InsufficientPermissionsError),
        }
    }

    pub async fn get_user(&self, db: &PgPool) -> Result<User> {
        match self {
            Self::User(u) => User::find_by_id(u.id, db).await,
            _ => Err(anyhow!("Authentication is not a user.").into()),
        }
    }

    pub async fn get_host(&self, db: &PgPool) -> Result<Host> {
        match self {
            Self::Host(token) => Host::find_by_token(token, db).await,
            _ => Err(anyhow!("Authentication is not a host.").into()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PwdResetInfo {
    pub token: String,
    #[validate(length(min = 8), must_match = "password_confirm")]
    pub password: String,
    pub password_confirm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(skip_serializing)]
    pub hashword: String,
    pub role: UserRole,
    #[serde(skip_serializing)]
    pub salt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub fee_bps: i64,
    pub staking_quota: i64,
    pub created_at: DateTime<Utc>,
}

impl From<PgRow> for User {
    fn from(row: PgRow) -> Self {
        User {
            id: row
                .try_get("id").expect("Couldn't try_get id for user."),
            email: row
                .try_get("email")
                .expect("Couldn't try_get email for user."),
            hashword: row
                .try_get("hashword")
                .expect("Couldn't try_get hashword for user."),
            role: row
                .try_get("role")
                .expect("Couldn't try_get role for user."),
            salt: row
                .try_get("salt")
                .expect("Couldn't try_get salt for user."),
            refresh: row
                .try_get("refresh")
                .expect("Couldn't try_get refresh for user."),
            token: row
                .try_get("token")
                .expect("Couldn't try_get token for user."),
            fee_bps: row
                .try_get("fee_bps")
                .expect("Couldn't try_get fee_bps for user."),
            staking_quota: row
                .try_get("staking_quota")
                .expect("Couldn't try_get staking_quota for user."),
            created_at: row
                .try_get("created_at")
                .expect("Couldn't try_get created_at for user."),
        }
    }
}

impl casbin_authorization::auth::Authorizable for User {
    fn get_role(&self) -> String {
        "user".to_string()
    }
}

#[tonic::async_trait]
impl FindableByToken for User {
    async fn find_by_token(token: &str, db: &PgPool) -> Result<Self> {
        Ok(sqlx::query("SELECT * FROM users WHERE token = $1")
            .bind(token)
            .map(Self::from)
            .fetch_one(db)
            .await?)
    }
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

    pub fn set_jwt(&mut self) -> Result<Self> {
        let auth_data = auth::AuthData {
            user_id: self.id,
            user_role: self.role.to_string(),
        };

        self.token = Some(auth::create_jwt(&auth_data)?);
        Ok(self.to_owned())
    }

    pub async fn reset_password(db: &PgPool, req: &PwdResetInfo) -> Result<User> {
        let _ = req
            .validate()
            .map_err(|e| ApiError::ValidationError(e.to_string()))?;

        match auth::validate_jwt(&req.token)? {
            auth::JwtValidationStatus::Valid(auth_data) => {
                let user = User::find_by_id(auth_data.user_id, db).await?;
                return user.update_password(&req.password, db).await;
            }
            _ => Err(ApiError::InsufficientPermissionsError),
        }
    }

    pub async fn email_reset_password(db: &PgPool, req: PasswordResetRequest) -> Result<()> {
        let user = User::find_by_email(&req.email, db).await?;

        let auth_data = auth::AuthData {
            user_id: user.id,
            user_role: user.role.to_string(),
        };

        let token = auth::create_temp_jwt(&auth_data)?;

        let p = Personalization::new(Email::new(&user.email));

        let subject = "Reset Password".to_string();
        let body = format!(
            r##"
            <h1>Password Reset</h1>
            <p>You have requested to reset your StakeJoy password. 
            Please visit <a href="https://console.stakejoy.com/reset?t={token}">
            https://console.stakejoy.com/reset?t={token}</a>.</p><br /><br /><p>Thank You!</p>"##
        );

        let sender = Sender::new(dotenv::var("SENDGRID_API_KEY").map_err(|_| {
            ApiError::UnexpectedError(anyhow!("Could not find SENDGRID_API_KEY in env."))
        })?);
        let m = Message::new(Email::new("StakeJoy <hello@stakejoy.com>"))
            .set_subject(&subject)
            .add_content(Content::new().set_content_type("text/html").set_value(body))
            .add_personalization(p);

        sender
            .send(&m)
            .await
            .map_err(|_| ApiError::UnexpectedError(anyhow!("Could not send email")))?;

        Ok(())
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
        sqlx::query_as::<_, Self>("SELECT * FROM users WHERE LOWER(email) = LOWER($1) limit 1")
            .bind(email)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_refresh(refresh: &str, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>("SELECT * FROM users WHERE refresh = $1 limit 1")
            .bind(refresh)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_id(id: Uuid, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>("SELECT * FROM users WHERE id = $1 limit 1")
            .bind(id)
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
            .map_err(ApiError::from)?
            .set_jwt();
        }

        Err(ApiError::ValidationError("Invalid password.".to_string()))
    }

    pub async fn create(user: UserRequest, db: &PgPool) -> Result<Self> {
        let _ = user
            .validate()
            .map_err(|e| ApiError::ValidationError(e.to_string()))?;

        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        if let Some(hashword) = argon2
            .hash_password_simple(user.password.as_bytes(), salt.as_str())?
            .hash
        {
            let mut tx = db.begin().await?;
            let user = sqlx::query_as::<_, Self>(
                "INSERT INTO users (email, hashword, salt, staking_quota, fee_bps) values (LOWER($1),$2,$3,$4,$5) RETURNING *",
            )
            .bind(user.email)
            .bind(hashword.to_string())
            .bind(salt.as_str())
            .bind(STAKE_QUOTA_DEFAULT)
            .bind(FEE_BPS_DEFAULT)
            .fetch_one(&mut tx)
            .await
            .map_err(ApiError::from)?
            .set_jwt();

            if let Ok(u) = &user {
                let org = sqlx::query_as::<_, Org>(
                    "INSERT INTO orgs (name, is_personal) values (LOWER($1), true) RETURNING *",
                )
                .bind(&u.email)
                .fetch_one(&mut tx)
                .await
                .map_err(ApiError::from)?;

                sqlx::query(
                    "INSERT INTO orgs_users (org_id, user_id, role) values($1, $2, 'owner')",
                )
                .bind(org.id)
                .bind(u.id)
                .execute(&mut tx)
                .await
                .map_err(ApiError::from)?;
            }
            tx.commit().await?;

            return user;
        }

        Err(ApiError::ValidationError("Invalid password.".to_string()))
    }

    pub async fn login(login: UserLoginRequest, db: &PgPool) -> Result<Self> {
        let mut user = Self::find_by_email(&login.email, db).await.map_err(|_e| {
            ApiError::InvalidAuthentication(anyhow!("Email or password is invalid."))
        })?;
        let _ = user.verify_password(&login.password)?;

        user.set_jwt()
    }

    pub async fn refresh(req: UserRefreshRequest, db: &PgPool) -> Result<User> {
        let mut user = Self::find_by_refresh(&req.refresh, db).await?;
        user.set_jwt()
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

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PasswordResetRequest {
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UserLoginRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
}

impl UserLoginRequest {
    pub async fn is_valid(&self, db: &PgPool) -> Result<bool> {
        let user = User::find_by_email(&self.email, db).await?;

        Ok(user.verify_password(&self.password).is_ok())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRefreshRequest {
    pub refresh: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserPayAddress {
    pub id: Uuid,
    pub pay_address: String,
}
