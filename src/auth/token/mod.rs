use anyhow::anyhow;
use axum::http::Request as HttpRequest;
use base64::DecodeError;
use chrono::Utc;
use diesel_async::AsyncPgConnection;
use hyper::header::AUTHORIZATION;
use jsonwebtoken as jwt;
use jsonwebtoken::errors::Error as JwtError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::Utf8Error;
use std::{env::VarError, str::FromStr};
use strum_macros::EnumIter;
use thiserror::Error;
use uuid::Uuid;

mod host_auth;
mod host_refresh;
mod invitation;
mod pwd_reset;
mod registration_confirmation;
mod user_auth;
mod user_refresh;

use crate::auth::expiration_provider::ExpirationProvider;
use crate::auth::key_provider::{KeyProvider, KeyProviderError};
use crate::auth::{FindableById, Identifiable};
use crate::models::{Host, User};
use crate::{Error, Result as ApiResult};
pub use {
    host_auth::HostAuthToken, host_refresh::HostRefreshToken, invitation::InvitationToken,
    pwd_reset::PwdResetToken, registration_confirmation::RegistrationConfirmationToken,
    user_auth::UserAuthToken, user_refresh::UserRefreshToken,
};

pub type TokenResult<T> = Result<T, TokenError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenRole {
    Admin,
    Guest,
    Service,
    User,
    OrgMember,
    OrgAdmin,
    PwdReset,
}

impl Display for TokenRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenRole::Admin => write!(f, "admin"),
            TokenRole::Guest => write!(f, "guest"),
            TokenRole::Service => write!(f, "service"),
            TokenRole::User => write!(f, "user"),
            TokenRole::OrgMember => write!(f, "org_member"),
            TokenRole::OrgAdmin => write!(f, "org_admin"),
            TokenRole::PwdReset => write!(f, "pwd_reset"),
        }
    }
}

impl FromStr for TokenRole {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(TokenRole::User),
            "service" => Ok(TokenRole::Service),
            "guest" => Ok(TokenRole::Guest),
            "admin" => Ok(TokenRole::Admin),
            "pwd_reset" => Ok(TokenRole::PwdReset),
            _ => Err(Error::UnexpectedError(anyhow!("Unknown role"))),
        }
    }
}

#[derive(Error, Debug)]
pub enum TokenError {
    #[error("Token is empty")]
    Empty,
    #[error("Token is incorrectly formatted")]
    Invalid,
    #[error("Token has expired")]
    Expired,
    #[error("Token couldn't be decoded: {0:?}")]
    EnDeCoding(#[from] JwtError),
    #[error("Env var not defined: {0:?}")]
    EnvVar(#[from] VarError),
    #[error("UTF-8 error: {0:?}")]
    Utf8(#[from] Utf8Error),
    #[error("JWT decoding error: {0:?}")]
    JwtDecoding(#[from] DecodeError),
    #[error("Provided key is invalid: {0:?}")]
    KeyError(#[from] KeyProviderError),
    #[error("Refresh token can't be read: {0:?}")]
    RefreshTokenError(#[from] anyhow::Error),
    #[error("Invitation token invalid: {0:?}")]
    Invitation(anyhow::Error),
    #[error("Invalid role in claim")]
    RoleError,
}

/// The type of token we are dealing with. We have various different types of token and they convey
/// various different permissions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    /// This is a "normal" login token obtained by sending the login credentials to
    /// `AuthenticationService.Login`.
    UserAuth,
    /// This is an auth token obtained by successfully claiming a HostProvision by sending the OTP to
    /// `HostService.Provision`.
    HostAuth,
    /// This is a dedicated refresh token. It can be used after the login token has expired to
    /// obtain a new refresh and login token pair.
    UserRefresh,
    /// This is a dedicated refresh token. It can be used after the login token has expired to
    /// obtain a new refresh and login token pair.
    HostRefresh,
    /// This is a password reset token. It is issued as a part of the password forgot/reset email
    /// and may be used _only_ to reset the user's password.
    PwdReset,
    /// This is the token used for confirming a new users registration
    RegistrationConfirmation,
    /// This is the token used for inviting users to an org
    Invitation,
    /// Token used for communication with cookbook
    Cookbook,
}

impl Display for TokenType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserAuth => write!(f, "user_auth"),
            Self::HostAuth => write!(f, "host_auth"),
            Self::UserRefresh => write!(f, "user_refresh"),
            Self::HostRefresh => write!(f, "host_refresh"),
            Self::PwdReset => write!(f, "pwd_reset"),
            Self::RegistrationConfirmation => write!(f, "registration_confirmation"),
            Self::Invitation => write!(f, "invitation"),
            Self::Cookbook => write!(f, "cookbook"),
        }
    }
}

/// The claims of the tokens. Each claim is a key-value pair
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenClaim {
    id: Uuid,
    exp: i64,
    token_type: TokenType,
    pub role: TokenRole,
    data: Option<HashMap<String, String>>,
}

impl TokenClaim {
    pub fn new(
        id: Uuid,
        exp: i64,
        token_type: TokenType,
        role: TokenRole,
        data: Option<HashMap<String, String>>,
    ) -> Self {
        Self {
            id,
            exp,
            token_type,
            role,
            data,
        }
    }
}

#[tonic::async_trait]
pub trait JwtToken: Sized + serde::Serialize {
    /* Getter common to all token types */
    fn get_expiration(&self) -> i64;
    fn get_id(&self) -> Uuid;

    fn try_new(claim: TokenClaim) -> TokenResult<Self>;

    fn token_type(&self) -> TokenType;

    /// Encode this instance to a JWT token string
    fn encode(&self) -> TokenResult<String> {
        let key = KeyProvider::get_secret(self.token_type())?;
        let secret = &key.value;
        let header = jwt::Header::new(jwt::Algorithm::HS512);
        let key = jwt::EncodingKey::from_secret(secret.as_ref());
        jwt::encode(&header, self, &key).map_err(TokenError::EnDeCoding)
    }

    /// Extract the JWT from given request
    fn from_request<B>(request: &HttpRequest<B>) -> TokenResult<Self>
    where
        Self: FromStr<Err = TokenError>,
    {
        extract_token(request).and_then(|s| Self::from_str(&s))
    }

    /// Create base64 hash value for encoded token
    fn to_base64(&self) -> ApiResult<String> {
        Ok(base64::encode(self.encode()?))
    }

    /// Try to retrieve user for given token
    async fn try_get_user(&self, id: Uuid, conn: &mut AsyncPgConnection) -> ApiResult<User> {
        match self.token_type() {
            TokenType::UserAuth
            | TokenType::UserRefresh
            | TokenType::RegistrationConfirmation
            | TokenType::PwdReset => User::find_by_id(id, conn).await,
            _ => Err(Error::UnexpectedError(anyhow!(
                "Cannot retrieve user from token of type {}",
                self.token_type().to_string()
            ))),
        }
    }

    /// Try to retrieve host for given token
    async fn try_get_host(&self, conn: &mut AsyncPgConnection) -> ApiResult<Host> {
        match self.token_type() {
            TokenType::HostAuth | TokenType::HostRefresh => {
                Host::find_by_id(self.get_id(), conn).await
            }
            _ => Err(Error::UnexpectedError(anyhow!(
                "Cannot retrieve host from token of type {}",
                self.token_type().to_string()
            ))),
        }
    }

    /// Create token for given resource
    fn create_token_for<T: Identifiable>(
        resource: &T,
        token_type: TokenType,
        role: TokenRole,
        data: Option<HashMap<String, String>>,
    ) -> TokenResult<Self> {
        let claim = TokenClaim::new(
            resource.get_id(),
            ExpirationProvider::expiration(token_type),
            token_type,
            role,
            data,
        );

        Self::try_new(claim)
    }

    /// Returns `true` if token has expired
    fn has_expired(&self) -> bool {
        let now = Utc::now().timestamp();

        now > self.get_expiration()
    }

    /// Decode token from encoded value
    fn from_encoded<T: JwtToken + DeserializeOwned>(
        encoded: &str,
        token_type: TokenType,
        validate_exp: bool,
    ) -> Result<T, TokenError> {
        let key = KeyProvider::get_secret(token_type)?;
        let secret = &key.value;
        let mut validation = jwt::Validation::new(jwt::Algorithm::HS512);

        validation.validate_exp = validate_exp;

        match jwt::decode::<T>(
            encoded,
            &jwt::DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        ) {
            Ok(token) => Ok(token.claims),
            Err(e) => {
                tracing::error!("Error decoding token: {e:?}");
                Err(TokenError::EnDeCoding(e))
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct UnknownToken {
    token_type: TokenType,
}

/// A token whose `token_type` is not known.
#[derive(Debug)]
pub enum AnyToken {
    UserAuth(UserAuthToken),
    HostAuth(HostAuthToken),
    PwdReset(PwdResetToken),
    UserRefresh(UserRefreshToken),
    HostRefresh(HostRefreshToken),
    RegistrationConfirmation(RegistrationConfirmationToken),
    Invitation(InvitationToken),
}

impl AnyToken {
    /// Deduces the correct of the token and then decodes the token according to that type.
    pub fn from_request<B>(req: &HttpRequest<B>) -> TokenResult<AnyToken> {
        use AnyToken::*;

        let token = extract_token(req)?;
        let payload = token.split('.').nth(1).ok_or(TokenError::Invalid)?;
        let decoded = base64::decode(payload).or(Err(TokenError::Invalid))?;
        let json: UnknownToken = serde_json::from_slice(&decoded).or(Err(TokenError::Invalid))?;
        let token = match json.token_type {
            TokenType::UserAuth => UserAuth(UserAuthToken::from_str(&token)?),
            TokenType::UserRefresh => UserRefresh(UserRefreshToken::from_str(&token)?),
            TokenType::HostAuth => HostAuth(HostAuthToken::from_str(&token)?),
            TokenType::HostRefresh => HostRefresh(HostRefreshToken::from_str(&token)?),
            TokenType::PwdReset => PwdReset(PwdResetToken::from_str(&token)?),
            TokenType::RegistrationConfirmation => {
                RegistrationConfirmation(RegistrationConfirmationToken::from_str(&token)?)
            }
            TokenType::Invitation => Invitation(InvitationToken::from_str(&token)?),
            TokenType::Cookbook => return Err(TokenError::Invalid),
        };

        Ok(token)
    }
}

fn extract_token<B>(req: &HttpRequest<B>) -> TokenResult<String> {
    let header = req
        .headers()
        .get(AUTHORIZATION)
        .ok_or(TokenError::Invalid)?;
    let header = header.to_str().map_err(|_| TokenError::Invalid)?;
    let header = header
        .strip_prefix("Bearer")
        .ok_or(TokenError::Invalid)?
        .trim();
    let token = base64::decode(header)?;
    let token = std::str::from_utf8(&token)?;

    Ok(token.to_owned())
}

/// Indicates the impl token is subject to be blacklisted once used
#[tonic::async_trait]
pub trait Blacklisted {
    /// Method needs to be called after validation and use
    async fn blacklist(&self, conn: &mut diesel_async::AsyncPgConnection) -> TokenResult<bool>;

    /// Return true if encoded token value can be found in blacklist table
    async fn is_blacklisted(
        &self,
        token: String,
        conn: &mut AsyncPgConnection,
    ) -> TokenResult<bool>;
}

pub fn determine_token_by_str(token: &str) -> TokenResult<TokenType> {
    let payload = token.split('.').nth(1).ok_or(TokenError::Invalid)?;
    let decoded = base64::decode(payload).or(Err(TokenError::Invalid))?;
    let json: UnknownToken = serde_json::from_slice(&decoded).or(Err(TokenError::Invalid))?;

    Ok(json.token_type)
}
