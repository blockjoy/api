use axum::extract::{Extension, Json};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tracing::{debug, warn};

use crate::auth::token::RequestToken;
use crate::models;

use super::mqtt::{self, AclRequest};

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
    debug!("MQTT auth payload: {payload:?}");
    (StatusCode::OK, Json("{}"))
}

pub async fn mqtt_acl(
    Extension(db): Extension<models::DbPool>,
    Json(payload): Json<AclRequest>,
) -> crate::Result<impl IntoResponse> {
    let bearer = match payload.username.parse()? {
        RequestToken::Bearer(bearer) => Ok(bearer),
        RequestToken::ApiKey(_) => Err(crate::Error::invalid_auth("Not bearer.")),
    }?;

    let claims = match db.context.auth.cipher.jwt.decode(&bearer) {
        Ok(claims) => claims,
        Err(err) => {
            warn!("Failed to decode JWT claims: {err}");
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json("{ \"message\": \"Unknown\"}"),
            ));
        }
    };

    let topic = payload.topic.parse()?;
    let mut conn = db.conn().await?;

    if mqtt::allow(claims, topic, &mut conn).await? {
        Ok((StatusCode::OK, Json("{}")))
    } else {
        Ok((StatusCode::FORBIDDEN, Json("{}")))
    }
}
