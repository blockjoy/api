use axum::response::IntoResponse;

use crate::{database, grpc::Status};

pub mod api_key;
pub mod chargebee;
pub mod health;
pub mod mqtt;
pub mod stripe;

pub(crate) struct Error {
    inner: serde_json::Value,
    status: hyper::StatusCode,
}

impl Error {
    pub const fn new(message: serde_json::Value, status: hyper::StatusCode) -> Self {
        Self {
            inner: message,
            status,
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (self.status, axum::Json(self.inner)).into_response()
    }
}

pub(crate) struct ErrorWrapper<T>(pub T);

impl<T: Into<Status>> IntoResponse for ErrorWrapper<T> {
    fn into_response(self) -> axum::response::Response {
        let error: Error = self.0.into().into();
        error.into_response()
    }
}

impl From<database::Error> for Error {
    fn from(err: database::Error) -> Self {
        tracing::error!("{err}");
        Self {
            inner: serde_json::json!({"message": err.to_string()}),
            status: hyper::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
