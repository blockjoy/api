use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::header::HeaderMap;
use axum::routing::{self, Router};
use axum::Json;
use diesel_async::scoped_futures::ScopedFutureExt;

use crate::config::Context;
use crate::database::Transaction;
use crate::grpc::{self, api, common};

use super::Error;

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/:id", routing::get(get))
        .route("/", routing::get(list))
        .route("/", routing::post(create))
        .route("/:id/report", routing::post(report_error))
        .route("/status", routing::post(report_status))
        .route("/config", routing::put(update_config))
        .route("/image", routing::put(upgrade_image))
        .route("/:id/start", routing::put(start))
        .route("/:id/stop", routing::put(stop))
        .route("/:id/restart", routing::put(restart))
        .route("/:id", routing::delete(delete))
        .with_state(context)
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::NodeServiceCreateRequest>,
) -> Result<Json<api::NodeServiceCreateResponse>, Error> {
    ctx.write(|write| grpc::node::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn get(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::NodeServiceGetRequest>,
) -> Result<Json<api::NodeServiceGetResponse>, Error> {
    ctx.read(|read| grpc::node::get(req, headers.into(), read).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::NodeServiceListRequest>,
) -> Result<Json<api::NodeServiceListResponse>, Error> {
    ctx.read(|read| grpc::node::list(req, headers.into(), read).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeServiceReportErrorRequest {
    created_by: common::Resource,
    message: String,
}

async fn report_error(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((node_id,)): Path<(String,)>,
    Json(req): Json<NodeServiceReportErrorRequest>,
) -> Result<Json<api::NodeServiceReportErrorResponse>, Error> {
    let req = api::NodeServiceReportErrorRequest {
        node_id,
        created_by: Some(req.created_by),
        message: req.message,
    };
    ctx.write(|write| grpc::node::report_error(req, headers.into(), write).scope_boxed())
        .await
}

async fn report_status(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::NodeServiceReportStatusRequest>,
) -> Result<Json<api::NodeServiceReportStatusResponse>, Error> {
    ctx.write(|write| grpc::node::report_status(req, headers.into(), write).scope_boxed())
        .await
}

async fn update_config(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::NodeServiceUpdateConfigRequest>,
) -> Result<Json<api::NodeServiceUpdateConfigResponse>, Error> {
    ctx.write(|write| grpc::node::update_config(req, headers.into(), write).scope_boxed())
        .await
}

async fn upgrade_image(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::NodeServiceUpgradeImageRequest>,
) -> Result<Json<api::NodeServiceUpgradeImageResponse>, Error> {
    ctx.write(|write| grpc::node::upgrade_image(req, headers.into(), write).scope_boxed())
        .await
}

async fn start(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::NodeServiceStartRequest>,
) -> Result<Json<api::NodeServiceStartResponse>, Error> {
    ctx.write(|write| grpc::node::start(req, headers.into(), write).scope_boxed())
        .await
}

async fn stop(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::NodeServiceStopRequest>,
) -> Result<Json<api::NodeServiceStopResponse>, Error> {
    ctx.write(|write| grpc::node::stop(req, headers.into(), write).scope_boxed())
        .await
}

async fn restart(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::NodeServiceRestartRequest>,
) -> Result<Json<api::NodeServiceRestartResponse>, Error> {
    ctx.write(|write| grpc::node::restart(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::NodeServiceDeleteRequest>,
) -> Result<Json<api::NodeServiceDeleteResponse>, Error> {
    ctx.write(|write| grpc::node::delete(req, headers.into(), write).scope_boxed())
        .await
}
