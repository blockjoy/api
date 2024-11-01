//! This module is used to construct `axum::Response` bodies to ensure
//! consistency in our API responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

const OK: StatusCode = StatusCode::OK;
const BAD_REQUEST: StatusCode = StatusCode::BAD_REQUEST;
const UNAUTHORIZED: StatusCode = StatusCode::UNAUTHORIZED;
const INTERNAL_SERVER_ERROR: StatusCode = StatusCode::INTERNAL_SERVER_ERROR;
const NOT_FOUND: StatusCode = StatusCode::NOT_FOUND;

#[derive(Clone, Copy)]
pub enum Message {
    BadParams,
    DbClosed,
    Empty,
    Failed,
    Unauthorized,
    NotFound,
    Custom(&'static str),
}

impl From<Message> for &'static str {
    fn from(message: Message) -> Self {
        use Message::*;
        match message {
            BadParams => "Bad params.",
            DbClosed => "DB connection is closed.",
            Empty => "",
            Failed => "Failed.",
            Unauthorized => "Unauthorized.",
            NotFound => "Not found.",
            Custom(msg) => msg,
        }
    }
}

pub fn ok() -> Response {
    (OK, Body::json(Message::Empty)).into_response()
}

pub fn ok_custom(msg: &'static str) -> Response {
    (OK, Body::json(Message::Custom(msg))).into_response()
}

pub fn bad_params() -> Response {
    (BAD_REQUEST, Body::json(Message::BadParams)).into_response()
}

pub fn unauthorized() -> Response {
    (UNAUTHORIZED, Body::json(Message::Unauthorized)).into_response()
}

pub fn failed() -> Response {
    (INTERNAL_SERVER_ERROR, Body::json(Message::Failed)).into_response()
}

pub fn not_found() -> Response {
    (NOT_FOUND, Body::json(Message::NotFound)).into_response()
}

pub fn db_closed() -> Response {
    (INTERNAL_SERVER_ERROR, Body::json(Message::DbClosed)).into_response()
}

#[derive(Serialize)]
struct Body {
    pub message: &'static str,
}

impl Body {
    fn json(message: Message) -> Json<Self> {
        Json(Body {
            message: message.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_status() {
        let ok = ok().into_response().status();
        assert_eq!(ok, OK);

        let unauthorized = unauthorized().into_response().status();
        assert_eq!(unauthorized, UNAUTHORIZED);

        let failed = failed().into_response().status();
        assert_eq!(failed, INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn http_body() {
        let resp = failed().into_response();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert_eq!(body, r#"{"message":"Failed."}"#);
    }
}
