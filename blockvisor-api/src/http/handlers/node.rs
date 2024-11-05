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
        .route("/:id", routing::get(get))
        .route("/", routing::get(list))
        .route("/", routing::post(create))
        .route("/", routing::put(upgrade))
        .route("/config", routing::put(update_config))
        .route("/status", routing::put(update_status))
        .route("/:id", routing::delete(delete))
        .route("/:id/report", routing::post(report))
        .route("/:id/start", routing::put(start))
        .route("/:id/stop", routing::put(stop))
        .route("/:id/restart", routing::put(restart))
        .with_state(context)
}

async fn get(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
) -> Result<Json<api::NodeServiceGetResponse>, super::Error> {
    let req = api::NodeServiceGetRequest { id };
    ctx.read(|read| grpc::node::get(req, headers.into(), read).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::NodeServiceListRequest>,
) -> Result<Json<api::NodeServiceListResponse>, super::Error> {
    ctx.read(|read| grpc::node::list(req, headers.into(), read).scope_boxed())
        .await
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::NodeServiceCreateRequest>,
) -> Result<Json<api::NodeServiceCreateResponse>, super::Error> {
    ctx.write(|write| grpc::node::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn upgrade(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::NodeServiceUpgradeRequest>,
) -> Result<Json<api::NodeServiceUpgradeResponse>, super::Error> {
    ctx.write(|write| grpc::node::upgrade(req, headers.into(), write).scope_boxed())
        .await
}

async fn update_config(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::NodeServiceUpdateConfigRequest>,
) -> Result<Json<api::NodeServiceUpdateConfigResponse>, super::Error> {
    ctx.write(|write| grpc::node::update_config(req, headers.into(), write).scope_boxed())
        .await
}

async fn update_status(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::NodeServiceUpdateStatusRequest>,
) -> Result<Json<api::NodeServiceUpdateStatusResponse>, super::Error> {
    ctx.write(|write| grpc::node::update_status(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
) -> Result<Json<api::NodeServiceDeleteResponse>, super::Error> {
    let req = api::NodeServiceDeleteRequest { id };
    ctx.write(|write| grpc::node::delete(req, headers.into(), write).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
struct NodeServiceReportRequest {
    user_id: String,
    message: String,
}

async fn report(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((node_id,)): Path<(String,)>,
    Json(req): Json<NodeServiceReportRequest>,
) -> Result<Json<api::NodeServiceReportResponse>, super::Error> {
    let req = api::NodeServiceReportRequest {
        user_id: req.user_id,
        node_id,
        message: req.message,
    };
    ctx.write(|write| grpc::node::report(req, headers.into(), write).scope_boxed())
        .await
}

async fn start(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
) -> Result<Json<api::NodeServiceStartResponse>, super::Error> {
    let req = api::NodeServiceStartRequest { id };
    ctx.write(|write| grpc::node::start(req, headers.into(), write).scope_boxed())
        .await
}

async fn stop(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
) -> Result<Json<api::NodeServiceStopResponse>, super::Error> {
    let req = api::NodeServiceStopRequest { id };
    ctx.write(|write| grpc::node::stop(req, headers.into(), write).scope_boxed())
        .await
}

async fn restart(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
) -> Result<Json<api::NodeServiceRestartResponse>, super::Error> {
    let req = api::NodeServiceRestartRequest { id };
    ctx.write(|write| grpc::node::restart(req, headers.into(), write).scope_boxed())
        .await
}
