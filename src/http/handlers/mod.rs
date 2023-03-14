use super::mqtt::MqttAclRequest;
use crate::auth::{determine_token_by_str, TokenType};
use crate::http::mqtt::{MqttAclPolicy, MqttHostPolicy, MqttUserPolicy};
use crate::models;
use anyhow::Context;
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

pub async fn mqtt_acl(
    Extension(db): Extension<models::DbPool>,
    Json(payload): Json<MqttAclRequest>,
) -> crate::Result<impl IntoResponse> {
    tracing::info!("Got acl payload: {payload:?}");

    // TODO: Remove the unwraps, just for testing
    match determine_token_by_str(&payload.username) {
        Ok(TokenType::UserAuth) => {
            let policy = MqttUserPolicy { db };
            if policy
                .allow(&payload.username, &payload.topic)
                .await
                .with_context(|| "Could not determine access")?
            {
                Ok((StatusCode::OK, Json("{}")))
            } else {
                Ok((StatusCode::FORBIDDEN, Json("{}")))
            }
        }
        Ok(TokenType::HostAuth) => {
            let policy = MqttHostPolicy;
            if policy
                .allow(&payload.username, &payload.topic)
                .await
                .with_context(|| "Could not determine access")?
            {
                Ok((StatusCode::OK, Json("{}")))
            } else {
                Ok((StatusCode::FORBIDDEN, Json("{}")))
            }
        }
        Ok(_) => Ok((
            StatusCode::FORBIDDEN,
            Json("{ \"message\": \"Not supported\"}"),
        )),
        Err(_) => Ok((
            StatusCode::UNAUTHORIZED,
            Json("{ \"message\": \"Unknown\"}"),
        )),
    }
}
