use crate::auth::TokenError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::num::TryFromIntError;
use tonic::Status;

pub type Result<T, E = ApiError> = std::result::Result<T, E>;

#[derive(thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    ValidationError(String),

    #[error("Record not found.")]
    NotFoundError(sqlx::Error),

    #[error("Duplicate resource conflict.")]
    DuplicateResource,

    #[error("invalid authentication credentials")]
    InvalidAuthentication(anyhow::Error),

    #[error("Insufficient permission.")]
    InsufficientPermissionsError,

    #[error("Error processing JWT")]
    JWTError(#[from] jsonwebtoken::errors::Error),

    #[error("Error related to JSON parsing or serialization: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Error converting to integer sizes: {0}")]
    IntegerError(#[from] TryFromIntError),

    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),

    #[error("UUID parse error: {0}")]
    UuidParseError(#[from] uuid::Error),

    #[error("No free IP available: {0}")]
    IpAssignmentError(sqlx::Error),

    #[error("Gateway IP mustn't be within the provided range: {0}")]
    IpGatewayError(anyhow::Error),

    #[error("Missing or invalid env param value: {0}")]
    EnvError(dotenv::Error),

    #[error("Error handling token: {0}")]
    TokenError(TokenError),

    #[error("Given user is not yet confirmed")]
    UserConfirmationError,

    #[error("Cannot parse IP address: {0}")]
    IpParseError(#[from] std::net::AddrParseError),
}

impl ApiError {
    pub fn validation(msg: impl std::fmt::Display) -> Self {
        Self::ValidationError(msg.to_string())
    }
}

impl std::fmt::Debug for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => Self::NotFoundError(e),
            sqlx::Error::Database(dbe) if dbe.to_string().contains("duplicate key value") => {
                Self::DuplicateResource
            }
            _ => Self::UnexpectedError(anyhow::Error::from(e)),
        }
    }
}

impl From<argon2::password_hash::Error> for ApiError {
    fn from(e: argon2::password_hash::Error) -> Self {
        Self::InvalidAuthentication(anyhow::Error::msg(e.to_string()))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status_code = match self {
            ApiError::ValidationError(_) => StatusCode::BAD_REQUEST,
            ApiError::NotFoundError(_) => StatusCode::NOT_FOUND,
            ApiError::DuplicateResource => StatusCode::CONFLICT,
            ApiError::InvalidAuthentication(_) => StatusCode::UNAUTHORIZED,
            ApiError::InsufficientPermissionsError => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let response = (status_code, Json(self.to_string())).into_response();
        tracing::error!("{:?}", response);
        response
    }
}

impl From<TokenError> for Status {
    fn from(e: TokenError) -> Self {
        Status::internal(format!("Token encode error {e:?}"))
    }
}

impl From<TokenError> for ApiError {
    fn from(e: TokenError) -> Self {
        ApiError::TokenError(e)
    }
}

pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    write!(f, "{}", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        write!(f, "\n\tCaused by:{}", cause)?;
        current = cause.source();
    }
    Ok(())
}
