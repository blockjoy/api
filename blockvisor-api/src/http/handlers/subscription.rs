use std::sync::Arc;

use axum::extract::{Path, State};
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
        .route("/:id", routing::get(get))
        .route("/", routing::get(list))
        .route("/", routing::put(update))
        .route("/:id", routing::delete(delete))
        .with_state(context)
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::OrgServiceCreateRequest>,
) -> Result<Json<api::OrgServiceCreateResponse>, super::Error> {
    ctx.write(|write| grpc::org::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn get(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
) -> Result<Json<api::OrgServiceGetResponse>, super::Error> {
    let req = api::OrgServiceGetRequest { id };
    ctx.read(|read| grpc::org::get(req, headers.into(), read).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::OrgServiceListRequest>,
) -> Result<Json<api::OrgServiceListResponse>, super::Error> {
    ctx.read(|read| grpc::org::list(req, headers.into(), read).scope_boxed())
        .await
}

async fn update(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::OrgServiceUpdateRequest>,
) -> Result<Json<api::OrgServiceUpdateResponse>, super::Error> {
    ctx.write(|write| grpc::org::update(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
) -> Result<Json<api::OrgServiceDeleteResponse>, super::Error> {
    let req = api::OrgServiceDeleteRequest { id };
    ctx.write(|write| grpc::org::delete(req, headers.into(), write).scope_boxed())
        .await
}
