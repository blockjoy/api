use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use diesel_async::pooled_connection::bb8::RunError;
use std::num::TryFromIntError;
use tonic::metadata::errors::InvalidMetadataValue;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    ValidationError(String),

    #[error("{0}")]
    NotFoundError(diesel::result::Error),

    #[error("Duplicate resource conflict on constraint {constraint}.")]
    DuplicateResource { constraint: String },

    #[error("Authentication error: {0}")]
    Auth(#[from] crate::auth::Error),

    #[error("Claims authentication error: {0}")]
    Claims(#[from] crate::auth::claims::Error),

    #[error("Token authentication error: {0}")]
    Token(#[from] crate::auth::token::Error),

    #[error("invalid authentication credentials: {0}")]
    InvalidAuthentication(String),

    #[error("{0}")]
    InsufficientPermissions(String),

    #[error("Error processing JWT: {0}")]
    Jwt(#[from] crate::auth::token::jwt::Error),

    #[error("Error processing refresh token: {0}")]
    RefreshToken(#[from] crate::auth::token::refresh::Error),

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

    #[error("Given user is not yet confirmed")]
    UserConfirmationError,

    #[error("Cannot parse IP address: {0}")]
    IpParseError(#[from] std::net::AddrParseError),

    /// This variant is used when an IP-address is not parseable in a CIDR format.
    #[error("Cannot parse CIDR address")]
    Cidr,

    #[error("{0}")]
    OtherIpParseError(#[from] ipnetwork::IpNetworkError),

    #[error("Config error: {0}")]
    Config(#[from] crate::config::Error),

    #[error("Error reading config key: {0}")]
    ConfigProvider(#[from] crate::config::provider::Error),

    #[error("Cookbook config error: {0}")]
    ConfigCookbook(#[from] crate::config::cookbook::Error),

    #[error("Struggles with receiving through channel: {0}")]
    ChannelError(#[from] tokio::sync::broadcast::error::RecvError),

    #[error("{0}")]
    InvalidArgument(tonic::Status),

    #[error("Mqtt error: {0}")]
    MqttError(#[from] rumqttc::ClientError),

    #[error("Cloudflare integration error: {0}")]
    Dns(#[from] crate::dns::Error),

    #[error("Could not select a matching host: {0}")]
    NoMatchingHostError(String),

    #[error("{0}")]
    BadMetaData(#[from] InvalidMetadataValue),

    #[error("{0}")]
    ToStrError(#[from] tonic::metadata::errors::ToStrError),

    #[error("Could not convert babel config to filter for node query {0}")]
    BabelConfigConvertError(String),

    #[error("One or more nodes could not be upgraded {0}")]
    UpgradeProcessError(String),

    #[error("Storage error: {0}")]
    S3(#[from] aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::get_object::GetObjectError>),
}

impl Error {
    pub fn validation(msg: impl std::fmt::Display) -> Self {
        Self::ValidationError(msg.to_string())
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
            tonic::Code::PermissionDenied => Error::InsufficientPermissions(status.to_string()),
            tonic::Code::InvalidArgument => Error::InvalidArgument(status),
            _ => Error::UnexpectedError(anyhow::anyhow!(status)),
        }
    }
}

impl From<Error> for tonic::Status {
    fn from(e: Error) -> Self {
        use Error::*;

        tracing::warn!("Returning {e}");

        let msg = format!("{e}");

        match e {
            ValidationError(_) => tonic::Status::invalid_argument(msg),
            NotFoundError(_) => tonic::Status::not_found(msg),
            DuplicateResource { .. } => tonic::Status::invalid_argument(msg),
            UuidParseError(_) | IpParseError(_) => tonic::Status::invalid_argument(msg),
            Auth(err) => err.into(),
            Token(err) => err.into(),
            InvalidAuthentication(_) => tonic::Status::unauthenticated(msg),
            InsufficientPermissions(_) => tonic::Status::permission_denied(msg),
            UserConfirmationError => tonic::Status::unauthenticated(msg),
            NoMatchingHostError(_) => tonic::Status::resource_exhausted(msg),
            InvalidArgument(s) => s,
            BabelConfigConvertError(s) => tonic::Status::invalid_argument(s),
            UpgradeProcessError(s) => tonic::Status::internal(s),
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
            Error::InsufficientPermissions(_) => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let response = (status_code, Json(self.to_string())).into_response();
        tracing::error!("{:?}", response);
        response
    }
}

fn error_chain_fmt(
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
