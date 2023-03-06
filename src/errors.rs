use crate::auth::key_provider::KeyProviderError;
use crate::auth::TokenError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use diesel_async::pooled_connection::bb8::RunError;
use std::num::TryFromIntError;
use tonic::Status;

pub type Result<T, E = ApiError> = std::result::Result<T, E>;

#[derive(thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    ValidationError(String),

    #[error("{0}")]
    NotFoundError(diesel::result::Error),

    #[error("Duplicate resource conflict on constraint {constraint}.")]
    DuplicateResource { constraint: String },

    #[error("invalid authentication credentials: {0}")]
    InvalidAuthentication(String),

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

    // #[error("No free IP available: {0}")]
    // IpAssignmentError(sqlx::Error),
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

    #[error("{0}")]
    OtherIpParseError(#[from] ipnetwork::IpNetworkError),

    #[error("Error reading key: {0}")]
    Key(#[from] KeyProviderError),

    #[error("Struggles with receiving through channel: {0}")]
    ChannelError(#[from] tokio::sync::broadcast::error::RecvError),

    #[error("User node quota exceeded")]
    NodeQuota,

    #[error("{0}")]
    InvalidArgument(tonic::Status),

    #[error("Mqtt error: {0}")]
    MqttError(#[from] rumqttc::ClientError),
}

impl ApiError {
    pub fn validation(msg: impl std::fmt::Display) -> Self {
        Self::ValidationError(msg.to_string())
    }

    pub fn db_enum(msg: impl std::fmt::Display) -> Self {
        Self::UnexpectedError(anyhow::anyhow!("Database enum struggle: `{msg}`"))
    }

    pub fn invalid_auth(msg: impl std::fmt::Display) -> Self {
        Self::InvalidAuthentication(msg.to_string())
    }
}

impl std::fmt::Debug for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

// impl From<sqlx::Error> for ApiError {
//     fn from(e: sqlx::Error) -> Self {
//         match e {
//             sqlx::Error::RowNotFound => Self::NotFoundError(e),
//             sqlx::Error::Database(dbe) if dbe.to_string().contains("duplicate key value") => {
//                 Self::DuplicateResource {
//                     // The string will look like:
//                     // 'duplicate key blabla violation "node_key_files_name_node_id_key"'
//                     // So we take the part after the first ", and before the second ".
//                     constraint: dbe
//                         .to_string()
//                         .split('"')
//                         .nth(1)
//                         .unwrap_or("No contraint was given")
//                         .to_owned(),
//                 }
//             }
//             _ => Self::UnexpectedError(anyhow::Error::from(e)),
//         }
//     }
// }

impl From<RunError> for ApiError {
    fn from(value: RunError) -> Self {
        anyhow::anyhow!("Database pool is not behaving: {value}").into()
    }
}

impl From<std::num::ParseIntError> for ApiError {
    fn from(value: std::num::ParseIntError) -> Self {
        anyhow::anyhow!("Could not parse integer: {value}").into()
    }
}

impl From<diesel::result::Error> for ApiError {
    fn from(value: diesel::result::Error) -> Self {
        use diesel::result::DatabaseErrorKind::*;
        use diesel::result::Error::*;

        match value {
            NotFound => Self::NotFoundError(value),
            DatabaseError(UniqueViolation, err) => Self::DuplicateResource {
                constraint: err.message().to_string(),
            },
            _ => Self::UnexpectedError(value.into()),
        }
    }
}

impl From<argon2::password_hash::Error> for ApiError {
    fn from(e: argon2::password_hash::Error) -> Self {
        Self::InvalidAuthentication(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status_code = match self {
            ApiError::ValidationError(_) => StatusCode::BAD_REQUEST,
            ApiError::NotFoundError(_) => StatusCode::NOT_FOUND,
            ApiError::DuplicateResource { .. } => StatusCode::CONFLICT,
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
    write!(f, "{e}")?;
    let mut current = e.source();
    while let Some(cause) = current {
        write!(f, "\n\tCaused by: {cause}")?;
        current = cause.source();
    }
    Ok(())
}
