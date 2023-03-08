use crate::auth::expiration_provider::ExpirationProvider;
use crate::auth::{
    token::TokenError, FindableById, Identifiable, JwtToken, TokenClaim, TokenRole, TokenType,
    UserAuthToken, UserRefreshToken,
};
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy_ui::LoginUserRequest;
use crate::mail::MailClient;
use anyhow::anyhow;
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use diesel::{dsl, prelude::*};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use rand::rngs::OsRng;
use uuid::Uuid;
use validator::Validate;

use super::schema::users;

#[derive(Debug, Clone, Queryable)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub hashword: String,
    pub salt: String,
    pub refresh: Option<String>,
    pub fee_bps: i64,
    pub created_at: DateTime<Utc>,
    pub staking_quota: i64,
    pub pay_address: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

type NotDeleted = dsl::Filter<users::table, dsl::IsNull<users::deleted_at>>;

impl User {
    /// Test if given `token` has expired and refresh it using the `refresh_token` if necessary
    pub async fn verify_and_refresh_auth_token(
        token: UserAuthToken,
        refresh_token: UserRefreshToken,
        conn: &mut AsyncPgConnection,
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
                Some(token.data),
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
            let user =
                Self::set_refresh(refresh_token.get_id(), &refresh_token.encode()?, conn).await?;

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

        Err(ApiError::invalid_auth("Invalid email or password."))
    }

    // pub async fn reset_password(_conn: &mut AsyncPgConnection, _req: &PwdResetInfo) -> Result<User> {
    //     // TODO: use new auth
    //     unimplemented!()
    // }

    pub async fn email_reset_password(&self, conn: &mut AsyncPgConnection) -> Result<()> {
        let client = MailClient::new();
        client.reset_password(self, conn).await
    }

    pub async fn find_all(conn: &mut AsyncPgConnection) -> Result<Vec<Self>> {
        let users = users::table.get_results(conn).await?;
        Ok(users)
    }

    pub async fn find_all_pay_address(conn: &mut AsyncPgConnection) -> Result<Vec<UserPayAddress>> {
        let addrs = Self::not_deleted()
            .filter(users::pay_address.is_not_null())
            .select((users::id, users::pay_address))
            .get_results(conn)
            .await?;
        Ok(addrs)
    }

    pub async fn find_by_email(email: &str, conn: &mut AsyncPgConnection) -> Result<Self> {
        let users = Self::not_deleted()
            .filter(super::lower(users::email).eq(&email.to_lowercase()))
            .get_result(conn)
            .await?;
        Ok(users)
    }

    pub async fn find_by_refresh(refresh: &str, conn: &mut AsyncPgConnection) -> Result<Self> {
        let users = Self::not_deleted()
            .filter(users::refresh.eq(refresh))
            .get_result(conn)
            .await?;
        Ok(users)
    }

    pub async fn update_password(
        &self,
        password: &str,
        conn: &mut AsyncPgConnection,
    ) -> Result<Self> {
        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        if let Some(hashword) = argon2
            .hash_password_simple(password.as_bytes(), salt.as_str())?
            .hash
        {
            let user = diesel::update(users::table.find(self.id))
                .set((
                    users::hashword.eq(hashword.to_string()),
                    users::salt.eq(salt.as_str()),
                ))
                .get_result(conn)
                .await?;
            Ok(user)
        } else {
            Err(ApiError::ValidationError("Invalid password.".to_string()))
        }
    }

    /// Check if user can be found by email, is confirmed and has provided a valid password
    pub async fn login(login: LoginUserRequest, conn: &mut AsyncPgConnection) -> Result<Self> {
        let user = Self::find_by_email(&login.email, conn)
            .await
            .map_err(|_e| ApiError::invalid_auth("Email or password is invalid."))?;

        if User::is_confirmed(user.id, conn).await? {
            match user.verify_password(&login.password) {
                Ok(_) => Ok(user),
                Err(e) => Err(e),
            }
        } else {
            Err(ApiError::UserConfirmationError)
        }
    }

    pub async fn set_refresh(
        id: uuid::Uuid,
        refresh_token: &str,
        conn: &mut AsyncPgConnection,
    ) -> Result<User> {
        let user = diesel::update(users::table.find(id))
            .set(users::refresh.eq(refresh_token))
            .get_result(conn)
            .await?;
        Ok(user)
    }

    pub async fn confirm(user_id: uuid::Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let target_user = Self::not_deleted()
            .find(user_id)
            .filter(users::confirmed_at.is_null());
        let user = diesel::update(target_user)
            .set(users::confirmed_at.eq(chrono::Utc::now()))
            .get_result(conn)
            .await?;
        Ok(user)
    }

    pub async fn is_confirmed(id: Uuid, conn: &mut AsyncPgConnection) -> Result<bool> {
        let is_confirmed = Self::not_deleted()
            .find(id)
            .select(users::confirmed_at.is_not_null())
            .get_result(conn)
            .await?;
        Ok(is_confirmed)
    }

    /// Mark user deleted if no more nodes belong to it
    pub async fn delete(id: Uuid, conn: &mut AsyncPgConnection) -> Result<()> {
        // TODO THOMAS: doesn't this mean that you cannot delete any users from organizations that
        // have nodes?
        // sqlx::query_as::<_, User>(
        //     r#"
        //     UPDATE users u SET
        //         deleted_at = now()
        //     WHERE id = $1
        //         AND (SELECT (COUNT(*) > 0) as delete_me from nodes LEFT JOIN orgs_users ou on u.id = ou.user_id)
        //     RETURNING *"#,
        // )
        // .bind(id)
        // .fetch_one(tx)
        // .await
        // .map_err(ApiError::from)

        diesel::update(users::table.find(id))
            .set(users::deleted_at.eq(chrono::Utc::now()))
            .execute(conn)
            .await?;
        Ok(())
    }

    pub fn preferred_language(&self) -> &str {
        // Needs to be done later, but we want to have some stub in place so we keep our code aware
        // of language differences.
        "en"
    }

    fn not_deleted() -> NotDeleted {
        users::table.filter(users::deleted_at.is_null())
    }
}

#[derive(Debug, Clone, Validate, Insertable)]
#[diesel(table_name = users)]
pub struct NewUser<'a> {
    #[validate(email)]
    email: &'a str,
    first_name: &'a str,
    last_name: &'a str,
    hashword: String,
    salt: String,
}

impl<'a> NewUser<'a> {
    pub fn new(
        email: &'a str,
        first_name: &'a str,
        last_name: &'a str,
        password: &'a str,
    ) -> Result<Self> {
        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        if let Some(hashword) = argon2
            .hash_password_simple(password.as_bytes(), &salt)?
            .hash
        {
            let create_user = Self {
                email,
                first_name,
                last_name,
                hashword: hashword.to_string(),
                salt: salt.as_str().to_owned(),
            };

            create_user
                .validate()
                .map_err(|e| ApiError::ValidationError(e.to_string()))?;
            Ok(create_user)
        } else {
            Err(ApiError::ValidationError("Invalid password.".to_string()))
        }
    }

    pub async fn create(self, conn: &mut AsyncPgConnection) -> Result<User> {
        let user: User = diesel::insert_into(users::table)
            .values(self)
            .get_result(conn)
            .await?;

        let org = super::NewOrg {
            name: "Personal",
            is_personal: true,
        };
        org.create(user.id, conn).await?;

        Ok(user)
    }
}

#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = users)]
pub struct UpdateUser<'a> {
    pub id: uuid::Uuid,
    pub first_name: Option<&'a str>,
    pub last_name: Option<&'a str>,
    pub fee_bps: Option<i64>,
    pub staking_quota: Option<i64>,
    pub refresh: Option<&'a str>,
}

impl<'a> UpdateUser<'a> {
    pub async fn update(self, conn: &mut AsyncPgConnection) -> Result<User> {
        let user = diesel::update(users::table.find(self.id))
            .set(self)
            .get_result(conn)
            .await?;

        Ok(user)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserLogin {
    pub(crate) id: Uuid,
    pub(crate) email: String,
    pub(crate) fee_bps: i64,
    pub(crate) staking_quota: i64,
    pub(crate) token: String,
}

#[axum::async_trait]
impl FindableById for User {
    async fn find_by_id(id: Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let user = User::not_deleted().find(id).get_result(conn).await?;
        Ok(user)
    }
}

impl Identifiable for User {
    fn get_id(&self) -> Uuid {
        self.id
    }
}

// #[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
// pub struct UserSummary {
//     pub id: Uuid,
//     pub email: String,
//     pub pay_address: Option<String>,
//     pub staking_quota: i64,
//     pub fee_bps: i64,
//     pub validator_count: i64,
//     pub rewards_total: i64,
//     pub invoices_total: i64,
//     pub payments_total: i64,
//     pub joined_at: DateTime<Utc>,
// }

// impl UserSummary {
//     pub fn balance(&self) -> i64 {
//         self.invoices_total - self.payments_total
//     }
// }
// #[derive(Debug, Clone, Serialize, Deserialize, Validate)]
// pub struct PasswordResetRequest {
//     #[validate(email)]
//     pub email: String,
// }

#[derive(Debug, Clone, Queryable)]
pub struct UserPayAddress {
    pub id: Uuid,
    // TODO: This field should not need to be optional
    pub pay_address: Option<String>,
}
