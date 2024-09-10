use axum::response::{IntoResponse, Response};
use hyper::StatusCode;
use serde_json::Value;

use crate::database;
use crate::grpc::Status;

pub mod api_key;
pub mod archive;
pub mod auth;
pub mod bundle;
pub mod discovery;
pub mod health;
pub mod host;
pub mod invitation;
pub mod metrics;
pub mod mqtt;
pub mod node;
pub mod org;
pub mod protocol;
pub mod stripe;
pub mod user;

pub(crate) struct Error {
    inner: Value,
    status: StatusCode,
}

impl Error {
    pub const fn new(message: Value, status: StatusCode) -> Self {
        Self {
            inner: message,
            status,
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.status, axum::Json(self.inner)).into_response()
    }
}

pub(crate) struct ErrorWrapper<T>(pub T);

impl<T: Into<Status>> IntoResponse for ErrorWrapper<T> {
    fn into_response(self) -> Response {
        let error: Error = self.0.into().into();
        error.into_response()
    }
}

impl From<database::Error> for Error {
    fn from(err: database::Error) -> Self {
        tracing::error!("{err}");
        Self {
            inner: serde_json::json!({"message": err.to_string()}),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
