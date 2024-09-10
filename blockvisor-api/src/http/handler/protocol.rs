use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::header::HeaderMap;
use axum::routing::{self, Router};
use axum::Json;
use diesel_async::scoped_futures::ScopedFutureExt;

use crate::config::Context;
use crate::database::Transaction;
use crate::grpc::api::protocol_service_get_protocol_request;
use crate::grpc::{self, api};

use super::Error;

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/", routing::post(add_protocol))
        .route("/", routing::get(list_protocols))
        .route("/:id", routing::get(get_protocol))
        .route("/key/:key", routing::get(get_protocol_by_key))
        .route("/:id", routing::put(update_protocol))
        .route("/version", routing::post(add_version))
        .route("/version", routing::get(list_versions))
        .route("/version/:id", routing::put(update_version))
        .route("/latest", routing::get(get_latest))
        .route("/pricing", routing::get(get_pricing))
        .route("/stats", routing::get(get_stats))
        .with_state(context)
}

async fn add_protocol(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::ProtocolServiceAddProtocolRequest>,
) -> Result<Json<api::ProtocolServiceAddProtocolResponse>, Error> {
    ctx.write(|write| grpc::protocol::add_protocol(req, headers.into(), write).scope_boxed())
        .await
}

async fn add_version(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::ProtocolServiceAddVersionRequest>,
) -> Result<Json<api::ProtocolServiceAddVersionResponse>, Error> {
    ctx.write(|write| grpc::protocol::add_version(req, headers.into(), write).scope_boxed())
        .await
}

async fn get_latest(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::ProtocolServiceGetLatestRequest>,
) -> Result<Json<api::ProtocolServiceGetLatestResponse>, Error> {
    ctx.read(|read| grpc::protocol::get_latest(req, headers.into(), read).scope_boxed())
        .await
}

async fn get_pricing(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::ProtocolServiceGetPricingRequest>,
) -> Result<Json<api::ProtocolServiceGetPricingResponse>, Error> {
    ctx.read(|read| grpc::protocol::get_pricing(req, headers.into(), read).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtocolServiceGetProtocolRequest {
    org_id: Option<String>,
}

async fn get_protocol(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((protocol_id,)): Path<(String,)>,
    Query(req): Query<ProtocolServiceGetProtocolRequest>,
) -> Result<Json<api::ProtocolServiceGetProtocolResponse>, Error> {
    let req = api::ProtocolServiceGetProtocolRequest {
        protocol: Some(protocol_service_get_protocol_request::Protocol::ProtocolId(
            protocol_id,
        )),
        org_id: req.org_id,
    };
    ctx.read(|read| grpc::protocol::get_protocol(req, headers.into(), read).scope_boxed())
        .await
}

async fn get_protocol_by_key(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((protocol_key,)): Path<(String,)>,
    Query(req): Query<ProtocolServiceGetProtocolRequest>,
) -> Result<Json<api::ProtocolServiceGetProtocolResponse>, Error> {
    let req = api::ProtocolServiceGetProtocolRequest {
        protocol: Some(protocol_service_get_protocol_request::Protocol::ProtocolKey(protocol_key)),
        org_id: req.org_id,
    };
    ctx.read(|read| grpc::protocol::get_protocol(req, headers.into(), read).scope_boxed())
        .await
}

async fn get_stats(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::ProtocolServiceGetStatsRequest>,
) -> Result<Json<api::ProtocolServiceGetStatsResponse>, Error> {
    ctx.read(|read| grpc::protocol::get_stats(req, headers.into(), read).scope_boxed())
        .await
}

async fn list_protocols(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::ProtocolServiceListProtocolsRequest>,
) -> Result<Json<api::ProtocolServiceListProtocolsResponse>, Error> {
    ctx.read(|read| grpc::protocol::list_protocols(req, headers.into(), read).scope_boxed())
        .await
}

async fn list_versions(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::ProtocolServiceListVersionsRequest>,
) -> Result<Json<api::ProtocolServiceListVersionsResponse>, Error> {
    ctx.read(|read| grpc::protocol::list_versions(req, headers.into(), read).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtocolServiceUpdateProtocolRequest {
    name: Option<String>,
    description: Option<String>,
    visibility: Option<i32>,
}

async fn update_protocol(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((protocol_id,)): Path<(String,)>,
    Query(req): Query<ProtocolServiceUpdateProtocolRequest>,
) -> Result<Json<api::ProtocolServiceUpdateProtocolResponse>, Error> {
    let req = api::ProtocolServiceUpdateProtocolRequest {
        protocol_id,
        name: req.name,
        description: req.description,
        visibility: req.visibility,
    };
    ctx.write(|write| grpc::protocol::update_protocol(req, headers.into(), write).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtocolServiceUpdateVersionRequest {
    sku_code: Option<String>,
    description: Option<String>,
    visibility: Option<i32>,
}

async fn update_version(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((protocol_version_id,)): Path<(String,)>,
    Query(req): Query<ProtocolServiceUpdateVersionRequest>,
) -> Result<Json<api::ProtocolServiceUpdateVersionResponse>, Error> {
    let req = api::ProtocolServiceUpdateVersionRequest {
        protocol_version_id,
        sku_code: req.sku_code,
        description: req.description,
        visibility: req.visibility,
    };
    ctx.write(|write| grpc::protocol::update_version(req, headers.into(), write).scope_boxed())
        .await
}
