#![allow(unused)]

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{self, Router};
use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tracing::{debug, error};

use crate::auth::resource::{OrgId, UserId};
use crate::config::Context;
use crate::database::{Transaction, WriteConn};
use crate::grpc::{self, api};
use crate::model::{self, User};
use crate::stripe::api::event;

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
    axum::Json(req): axum::Json<api::ApiKeyServiceCreateRequest>,
) -> Result<axum::Json<api::ApiKeyServiceCreateResponse>, super::Error> {
    ctx.write(|write| grpc::api_key::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    axum::Json(req): axum::Json<api::ApiKeyServiceListRequest>,
) -> Result<axum::Json<api::ApiKeyServiceListResponse>, super::Error> {
    ctx.read(|read| grpc::api_key::list(req, headers.into(), read).scope_boxed())
        .await
}

async fn update(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    axum::Json(req): axum::Json<api::ApiKeyServiceUpdateRequest>,
) -> Result<axum::Json<api::ApiKeyServiceUpdateResponse>, super::Error> {
    ctx.write(|write| grpc::api_key::update(req, headers.into(), write).scope_boxed())
        .await
}

async fn regenerate(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    axum::Json(req): axum::Json<api::ApiKeyServiceRegenerateRequest>,
) -> Result<axum::Json<api::ApiKeyServiceRegenerateResponse>, super::Error> {
    ctx.write(|write| grpc::api_key::regenerate(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    axum::Json(req): axum::Json<api::ApiKeyServiceDeleteRequest>,
) -> Result<axum::Json<api::ApiKeyServiceDeleteResponse>, super::Error> {
    ctx.write(|write| grpc::api_key::delete(req, headers.into(), write).scope_boxed())
        .await
}
