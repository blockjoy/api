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
        .route("/", routing::post(create))
        .route("/:id", routing::get(get))
        .route("/", routing::get(list))
        .route("/:id", routing::put(update))
        .route("/:id/", routing::delete(delete))
        .route("/:id/start", routing::put(start))
        .route("/:id/stop", routing::put(stop))
        .route("/:id/restart", routing::put(restart))
        .route("/regions", routing::get(regions))
        .with_state(context)
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::HostServiceCreateRequest>,
) -> Result<Json<api::HostServiceCreateResponse>, Error> {
    ctx.write(|write| grpc::host::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn get(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((host_id,)): Path<(String,)>,
) -> Result<Json<api::HostServiceGetResponse>, Error> {
    let req = api::HostServiceGetRequest { host_id };
    ctx.read(|read| grpc::host::get(req, headers.into(), read).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::HostServiceListRequest>,
) -> Result<Json<api::HostServiceListResponse>, Error> {
    ctx.read(|read| grpc::host::list(req, headers.into(), read).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HostServiceUpdateRequest {
    network_name: Option<String>,
    display_name: Option<String>,
    region: Option<String>,
    os: Option<String>,
    os_version: Option<String>,
    bv_version: Option<String>,
    cpu_cores: Option<u64>,
    memory_bytes: Option<u64>,
    disk_bytes: Option<u64>,
    schedule_type: Option<i32>,
    update_tags: Option<common::UpdateTags>,
}

async fn update(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((host_id,)): Path<(String,)>,
    Json(req): Json<HostServiceUpdateRequest>,
) -> Result<Json<api::HostServiceUpdateResponse>, Error> {
    let req = api::HostServiceUpdateRequest {
        host_id,
        network_name: req.network_name,
        display_name: req.display_name,
        region: req.region,
        os: req.os,
        os_version: req.os_version,
        bv_version: req.bv_version,
        cpu_cores: req.cpu_cores,
        memory_bytes: req.memory_bytes,
        disk_bytes: req.disk_bytes,
        schedule_type: req.schedule_type,
        update_tags: req.update_tags,
    };
    ctx.write(|write| grpc::host::update(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((host_id,)): Path<(String,)>,
) -> Result<Json<api::HostServiceDeleteResponse>, Error> {
    let req = api::HostServiceDeleteRequest { host_id };
    ctx.write(|write| grpc::host::delete(req, headers.into(), write).scope_boxed())
        .await
}

async fn start(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((host_id,)): Path<(String,)>,
) -> Result<Json<api::HostServiceStartResponse>, Error> {
    let req = api::HostServiceStartRequest { host_id };
    ctx.write(|write| grpc::host::start(req, headers.into(), write).scope_boxed())
        .await
}

async fn stop(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((host_id,)): Path<(String,)>,
) -> Result<Json<api::HostServiceStopResponse>, Error> {
    let req = api::HostServiceStopRequest { host_id };
    ctx.write(|write| grpc::host::stop(req, headers.into(), write).scope_boxed())
        .await
}

async fn restart(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((host_id,)): Path<(String,)>,
) -> Result<Json<api::HostServiceRestartResponse>, Error> {
    let req = api::HostServiceRestartRequest { host_id };
    ctx.write(|write| grpc::host::restart(req, headers.into(), write).scope_boxed())
        .await
}

async fn regions(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::HostServiceRegionsRequest>,
) -> Result<Json<api::HostServiceRegionsResponse>, Error> {
    ctx.read(|read| grpc::host::regions(req, headers.into(), read).scope_boxed())
        .await
}
