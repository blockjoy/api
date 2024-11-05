pub mod handlers;
pub mod response;

use std::sync::Arc;

use axum::Router;
use axum_tracing_opentelemetry::middleware::OtelAxumLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::config::Context;

use self::handlers::{
    api_key, auth, blockchain, blockchain_archive, bundle, chargebee, discovery, health, host,
    invitation, kernel, metrics, mqtt, node, org, stripe, subscription, user,
};

pub fn router(context: &Arc<Context>) -> Router {
    let cors = CorsLayer::new()
        .allow_headers(Any)
        .allow_methods(Any)
        .allow_origin(Any);

    Router::new()
        .layer(cors)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(OtelAxumLayer::default())
        // These are the endpoints that are also gRPC handlers
        .nest("/v1/api_key", api_key::router(context.clone()))
        .nest("/v1/auth", auth::router(context.clone()))
        .nest("/v1/blockchain", blockchain::router(context.clone()))
        .nest(
            "/v1/blockchain_archive",
            blockchain_archive::router(context.clone()),
        )
        .nest("/v1/bundle", bundle::router(context.clone()))
        .nest("/v1/chargebee", chargebee::router(context.clone()))
        .nest("/v1/discovery", discovery::router(context.clone()))
        .nest("/v1/host", host::router(context.clone()))
        .nest("/v1/invitation", invitation::router(context.clone()))
        .nest("/v1/kernel", kernel::router(context.clone()))
        .nest("/v1/metrics", metrics::router(context.clone()))
        .nest("/v1/node", node::router(context.clone()))
        .nest("/v1/org", org::router(context.clone()))
        .nest("/v1/subscription", subscription::router(context.clone()))
        .nest("/v1/user", user::router(context.clone()))
        // These are utility endpoints that are not accessible through the gRPC API
        .nest("/v1/stripe", stripe::router(context.clone()))
        .nest("/mqtt", mqtt::router(context.clone()))
        .nest("/chargebee", chargebee::router(context.clone()))
        .merge(health::router(context.clone()))
}
