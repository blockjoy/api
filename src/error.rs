use crate::auth::key_provider::KeyProviderError;
use crate::auth::TokenError;
use crate::cloudflare::DnsError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use diesel_async::pooled_connection::bb8::RunError;
use std::num::TryFromIntError;
use tonic::Status;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(thiserror::Error)]
pub enum Error {
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

    #[error("Cannot parse CIDR address")]
    Cidr,

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

    #[error("Cloudflare integration error: {0}")]
    DnsError(#[from] DnsError),

    #[error("Could not select a matching host")]
    NoMatchingHostError(String),
}

impl Error {
    pub fn validation(msg: impl std::fmt::Display) -> Self {
        Self::ValidationError(msg.to_string())
    }

    pub fn db_enum(msg: impl std::fmt::Display) -> Self {
        Self::UnexpectedError(anyhow::anyhow!("Database enum struggle: `{msg}`"))
    }

    pub fn invalid_auth(msg: impl std::fmt::Display) -> Self {
        Self::InvalidAuthentication(msg.to_string())
    }

    pub fn unexpected(msg: impl std::fmt::Display) -> Self {
        Self::UnexpectedError(anyhow::anyhow!("{msg}"))
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl From<RunError> for Error {
    fn from(value: RunError) -> Self {
        anyhow::anyhow!("Database pool is not behaving: {value}").into()
    }
}

impl From<tonic::Status> for Error {
    fn from(status: tonic::Status) -> Self {
        match status.code() {
            tonic::Code::Unauthenticated => Error::InvalidAuthentication(status.to_string()),
            tonic::Code::PermissionDenied => Error::InsufficientPermissionsError,
            tonic::Code::InvalidArgument => Error::InvalidArgument(status),
            _ => Error::UnexpectedError(anyhow::anyhow!(status)),
        }
    }
}

impl From<Error> for tonic::Status {
    fn from(e: Error) -> Self {
        use Error::*;

        let msg = format!("{e:?}");

        match e {
            ValidationError(_) => tonic::Status::invalid_argument(msg),
            NotFoundError(_) => tonic::Status::not_found(msg),
            DuplicateResource { .. } => tonic::Status::invalid_argument(msg),
            InvalidAuthentication(_) => tonic::Status::unauthenticated(msg),
            InsufficientPermissionsError => tonic::Status::permission_denied(msg),
            UuidParseError(_) | IpParseError(_) => tonic::Status::invalid_argument(msg),
            NoMatchingHostError(_) => tonic::Status::resource_exhausted(msg),
            InvalidArgument(s) => s,
            _ => tonic::Status::internal(msg),
        }
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(value: std::num::ParseIntError) -> Self {
        anyhow::anyhow!("Could not parse integer: {value}").into()
    }
}

impl From<diesel::result::Error> for Error {
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

impl From<argon2::password_hash::Error> for Error {
    fn from(e: argon2::password_hash::Error) -> Self {
        Self::InvalidAuthentication(e.to_string())
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status_code = match self {
            Error::ValidationError(_) => StatusCode::BAD_REQUEST,
            Error::NotFoundError(_) => StatusCode::NOT_FOUND,
            Error::DuplicateResource { .. } => StatusCode::CONFLICT,
            Error::InvalidAuthentication(_) => StatusCode::UNAUTHORIZED,
            Error::InsufficientPermissionsError => StatusCode::FORBIDDEN,
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

impl From<TokenError> for Error {
    fn from(e: TokenError) -> Self {
        Error::TokenError(e)
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
