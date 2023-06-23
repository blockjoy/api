use super::mqtt::{MqttAclRequest, MqttPolicy};
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

    match db.context.cipher.jwt.decode(&payload.username) {
        Ok(token) => {
            let policy = MqttPolicy { db };
            if policy
                .allow(token, &payload.topic)
                .await
                .with_context(|| "Could not determine access")?
            {
                Ok((StatusCode::OK, Json("{}")))
            } else {
                Ok((StatusCode::FORBIDDEN, Json("{}")))
            }
        }
        Err(_) => Ok((
            StatusCode::UNAUTHORIZED,
            Json("{ \"message\": \"Unknown\"}"),
        )),
    }
}
