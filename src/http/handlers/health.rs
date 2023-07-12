//! Health handler used to indicate system status.

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, Router};

use crate::config::Context;
use crate::http::response;

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/health", get(health))
        .with_state(context)
}

async fn health(State(ctx): State<Arc<Context>>) -> Response {
    if ctx.pool.is_open() {
        response::ok().into_response()
    } else {
        response::db_closed().into_response()
    }
}
