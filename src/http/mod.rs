use crate::http::routes::unauthenticated_routes;
use crate::server::DbPool;
use axum::{Extension, Router};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub mod handlers;
pub mod routes;

pub async fn server(db: DbPool) -> Router {
    unauthenticated_routes()
        // Common layers need to be added first to make it available to ALL routes
        .layer(
            CorsLayer::new()
                .allow_headers(Any)
                .allow_methods(Any)
                .allow_origin(Any),
        )
        .layer(CompressionLayer::new())
        .layer(Extension(db))
        .layer(TraceLayer::new_for_http())
}

pub struct HttpLoginUserRequest {
    pub(crate) email: String,
    pub(crate) pwd: String,
}

#[allow(dead_code)]
pub struct ClaimHostProvisionRequest {
    pub(crate) otp: String,
}
