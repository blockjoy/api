use crate::http::handlers::*;
use axum::routing::{get, post};
use axum::Router;

pub fn unauthenticated_routes() -> Router {
    let mqtt_router = Router::new()
        // mqtt/auth
        // -> { username: %u => JWT TOKEN }
        .route("/acl", post(mqtt_acl))
        .route("/auth", post(mqtt_auth));

    Router::new()
        .route("/health", get(health))
        .nest("/mqtt", mqtt_router)
}
