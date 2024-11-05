use std::sync::Arc;

use axum::extract::{Path, Query, State};
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
        .route("/:id/start", routing::put(start))
        .route("/:id/stop", routing::put(stop))
        .route("/:id/restart", routing::put(restart))
        .route("/regions", routing::get(regions))
        .with_state(context)
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::HostServiceCreateRequest>,
) -> Result<Json<api::HostServiceCreateResponse>, super::Error> {
    ctx.write(|write| grpc::host::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn get(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
) -> Result<Json<api::HostServiceGetResponse>, super::Error> {
    let req = api::HostServiceGetRequest { id };
    ctx.read(|read| grpc::host::get(req, headers.into(), read).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::HostServiceListRequest>,
) -> Result<Json<api::HostServiceListResponse>, super::Error> {
    ctx.read(|read| grpc::host::list(req, headers.into(), read).scope_boxed())
        .await
}

async fn update(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::HostServiceUpdateRequest>,
) -> Result<Json<api::HostServiceUpdateResponse>, super::Error> {
    ctx.write(|write| grpc::host::update(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
) -> Result<Json<api::HostServiceDeleteResponse>, super::Error> {
    let req = api::HostServiceDeleteRequest { id };
    ctx.write(|write| grpc::host::delete(req, headers.into(), write).scope_boxed())
        .await
}

async fn start(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,

    Path((id,)): Path<(String,)>,
) -> Result<Json<api::HostServiceStartResponse>, super::Error> {
    let req = api::HostServiceStartRequest { id };
    ctx.write(|write| grpc::host::start(req, headers.into(), write).scope_boxed())
        .await
}

async fn stop(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,

    Path((id,)): Path<(String,)>,
) -> Result<Json<api::HostServiceStopResponse>, super::Error> {
    let req = api::HostServiceStopRequest { id };
    ctx.write(|write| grpc::host::stop(req, headers.into(), write).scope_boxed())
        .await
}

async fn restart(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,

    Path((id,)): Path<(String,)>,
) -> Result<Json<api::HostServiceRestartResponse>, super::Error> {
    let req = api::HostServiceRestartRequest { id };
    ctx.write(|write| grpc::host::restart(req, headers.into(), write).scope_boxed())
        .await
}

async fn regions(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::HostServiceRegionsRequest>,
) -> Result<Json<api::HostServiceRegionsResponse>, super::Error> {
    ctx.read(|read| grpc::host::regions(req, headers.into(), read).scope_boxed())
        .await
}
