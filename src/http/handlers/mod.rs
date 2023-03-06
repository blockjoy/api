use super::mqtt::MqttAclRequest;
use crate::auth::{determine_token_by_str, TokenType};
use crate::http::mqtt::{MqttAclPolicy, MqttHostPolicy, MqttUserPolicy};
use crate::models;
use axum::extract::{Extension, Json};
use axum::http::StatusCode;
use axum::response::IntoResponse;

/// Health handler used indicating system status
/// Returns empty message (assuming all is working properly).
/// DB extension is passed in to check DB status
pub async fn health(Extension(db): Extension<models::DbPool>) -> impl IntoResponse {
    if db.is_closed() {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json("DB connection is closed"),
        )
    } else {
        (StatusCode::OK, Json(""))
    }
}

pub async fn mqtt_auth(Json(payload): Json<serde_json::Value>) -> impl IntoResponse {
    tracing::info!("Value is {payload:?}");
    (StatusCode::OK, Json("{}"))
}

pub async fn mqtt_acl(Json(payload): Json<MqttAclRequest>) -> impl IntoResponse {
    tracing::info!("Got acl payload: {payload:?}");

    // TODO: Remove the unwraps, just for testing
    match determine_token_by_str(&payload.username) {
        Ok(TokenType::UserAuth) => {
            if MqttUserPolicy::allow(&payload.username, payload.topic).unwrap() {
                (StatusCode::OK, Json("{}"))
            } else {
                (StatusCode::FORBIDDEN, Json("{}"))
            }
        }
        Ok(TokenType::HostAuth) => {
            if MqttHostPolicy::allow(&payload.username, payload.topic).unwrap() {
                (StatusCode::OK, Json("{}"))
            } else {
                (StatusCode::FORBIDDEN, Json("{}"))
            }
        }
        Ok(_) => (
            StatusCode::IM_A_TEAPOT,
            Json("{ \"message\": \"Not supported\"}"),
        ),
        Err(_) => (
            StatusCode::EXPECTATION_FAILED,
            Json("{ \"message\": \"Unknown\"}"),
        ),
    }
}
