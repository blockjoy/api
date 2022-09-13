use axum::http::Request as HttpRequest;
use base64::DecodeError;
use http::header::AUTHORIZATION;
use jsonwebtoken as jwt;
use jsonwebtoken::errors::Error as JwtError;
use serde::{Deserialize, Serialize};
use std::str::Utf8Error;
use std::{env::VarError, str::FromStr};
use thiserror::Error;
use uuid::Uuid;

mod auth;
mod pwd_reset;
pub use {auth::AuthToken, pwd_reset::PwdResetToken};

pub type TokenResult<T> = Result<T, TokenError>;

pub trait Identifier {
    fn get_id(&self) -> Uuid;
}

#[derive(Error, Debug)]
pub enum TokenError {
    #[error("Token is empty")]
    Empty,
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
}

/// The type of token we are dealing with. We have various different types of token and they convey
/// various different permissions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "token_type", rename_all = "snake_case")]
pub enum TokenType {
    /// This is a "normal" login token obtained by sending the login credentials to
    /// `AuthenticationService.Login`.
    Login,
    /// This is a dedicated refresh token. It can be used after the login token has expired to
    /// obtain a new refresh and login token pair.
    Refresh,
    /// This is a password reset token. It is issued as a part of the password forgot/reset email
    /// and may be used _only_ to reset the user's password.
    PwdReset,
}

/// The type of entity that is granted some permission through this token.
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum TokenHolderType {
    /// This means that the token authenticates a host machine.
    Host,
    /// This means that the token authenticates a user of our web console.
    User,
}

impl TokenHolderType {
    pub fn id_field(&self) -> &'static str {
        match self {
            Self::User => "user_id",
            Self::Host => "host_id",
        }
    }
}

pub trait JwtToken: Sized + serde::Serialize {
    fn new(id: Uuid, exp: i64, holder_type: TokenHolderType) -> Self;

    fn token_holder(&self) -> TokenHolderType;

    /// Encode this instance to a JWT token string
    fn encode(&self) -> TokenResult<String> {
        let secret = Self::get_secret()?;
        let header = jwt::Header::new(jwt::Algorithm::HS512);
        let key = jwt::EncodingKey::from_secret(secret.as_ref());
        jwt::encode(&header, self, &key).map_err(super::TokenError::EnDeCoding)
    }

    /// Create new JWT token from given request
    fn new_for_request<B>(request: &HttpRequest<B>) -> TokenResult<Self>
    where
        Self: FromStr<Err = TokenError>,
    {
        let token = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|hv| hv.to_str().ok())
            .and_then(|hv| hv.strip_prefix("Bearer"))
            .map(|tkn| tkn.trim())
            .unwrap_or("");
        let clear_token = base64::decode(token)?;
        let token = std::str::from_utf8(&clear_token)?;

        Self::from_str(token)
    }

    fn get_secret() -> super::TokenResult<String>;
}
