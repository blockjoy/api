use std::sync::Arc;

use axum::extract::State;
use axum::routing::{self, Router};
use axum::Json;
use diesel_async::scoped_futures::ScopedFutureExt;

use crate::config::Context;
use crate::database::Transaction;
use crate::grpc::{self, api};

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/", routing::post(create))
        .route("/", routing::get(list))
        .route("/", routing::put(update))
        .route("/regenerate", routing::post(regenerate))
        .route("/", routing::delete(delete))
        .with_state(context)
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::ApiKeyServiceCreateRequest>,
) -> Result<Json<api::ApiKeyServiceCreateResponse>, super::Error> {
    ctx.write(|write| grpc::api_key::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::ApiKeyServiceListRequest>,
) -> Result<Json<api::ApiKeyServiceListResponse>, super::Error> {
    ctx.read(|read| grpc::api_key::list(req, headers.into(), read).scope_boxed())
        .await
}

async fn update(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::ApiKeyServiceUpdateRequest>,
) -> Result<Json<api::ApiKeyServiceUpdateResponse>, super::Error> {
    ctx.write(|write| grpc::api_key::update(req, headers.into(), write).scope_boxed())
        .await
}

async fn regenerate(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::ApiKeyServiceRegenerateRequest>,
) -> Result<Json<api::ApiKeyServiceRegenerateResponse>, super::Error> {
    ctx.write(|write| grpc::api_key::regenerate(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::ApiKeyServiceDeleteRequest>,
) -> Result<Json<api::ApiKeyServiceDeleteResponse>, super::Error> {
    ctx.write(|write| grpc::api_key::delete(req, headers.into(), write).scope_boxed())
        .await
}
