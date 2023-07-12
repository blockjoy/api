//! This module is used to construct `axum::Response` bodies to ensure
//! consistency in our API responses.

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

#[derive(Clone, Copy)]
pub enum Message {
    BadParams,
    DbClosed,
    Empty,
    Failed,
    Unauthorized,
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
        }
    }
}

pub fn ok() -> impl IntoResponse {
    (StatusCode::OK, Body::json(Message::Empty))
}

pub fn bad_params() -> impl IntoResponse {
    (StatusCode::BAD_REQUEST, Body::json(Message::BadParams))
}

pub fn unauthorized() -> impl IntoResponse {
    (StatusCode::UNAUTHORIZED, Body::json(Message::Unauthorized))
}

pub fn failed() -> impl IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Body::json(Message::Failed),
    )
}

pub fn db_closed() -> impl IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Body::json(Message::DbClosed),
    )
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
        assert_eq!(ok, StatusCode::OK);

        let unauthorized = unauthorized().into_response().status();
        assert_eq!(unauthorized, StatusCode::UNAUTHORIZED);

        let failed = failed().into_response().status();
        assert_eq!(failed, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn http_body() {
        let resp = failed().into_response();
        let bytes = hyper::body::to_bytes(resp).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert_eq!(body, r#"{"message":"Failed."}"#);
    }
}
