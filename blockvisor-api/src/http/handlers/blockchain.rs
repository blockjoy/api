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
        .route("/image", routing::get(get_image))
        .route("/plugin", routing::get(get_plugin))
        .route("/requirements", routing::get(get_requirements))
        .route("/", routing::put(list))
        .route("/image", routing::put(list_image_versions))
        .route("/node_type", routing::post(add_node_type))
        .route("/version", routing::post(add_version))
        .route("/pricing", routing::get(pricing))
        .with_state(context)
}

#[derive(serde::Deserialize)]
struct BlockchainServiceGetRequest {
    org_id: Option<String>,
}

async fn get(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
    Query(req): Query<BlockchainServiceGetRequest>,
) -> Result<Json<api::BlockchainServiceGetResponse>, super::Error> {
    let req = api::BlockchainServiceGetRequest {
        id,
        org_id: req.org_id,
    };
    ctx.read(|read| grpc::blockchain::get(req, headers.into(), read).scope_boxed())
        .await
}

async fn get_image(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::BlockchainServiceGetImageRequest>,
) -> Result<Json<api::BlockchainServiceGetImageResponse>, super::Error> {
    ctx.read(|read| grpc::blockchain::get_image(req, headers.into(), read).scope_boxed())
        .await
}

async fn get_plugin(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::BlockchainServiceGetPluginRequest>,
) -> Result<Json<api::BlockchainServiceGetPluginResponse>, super::Error> {
    ctx.read(|read| grpc::blockchain::get_plugin(req, headers.into(), read).scope_boxed())
        .await
}

async fn get_requirements(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::BlockchainServiceGetRequirementsRequest>,
) -> Result<Json<api::BlockchainServiceGetRequirementsResponse>, super::Error> {
    ctx.read(|read| grpc::blockchain::get_requirements(req, headers.into(), read).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::BlockchainServiceListRequest>,
) -> Result<Json<api::BlockchainServiceListResponse>, super::Error> {
    ctx.read(|read| grpc::blockchain::list(req, headers.into(), read).scope_boxed())
        .await
}

async fn list_image_versions(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::BlockchainServiceListImageVersionsRequest>,
) -> Result<Json<api::BlockchainServiceListImageVersionsResponse>, super::Error> {
    ctx.read(|read| grpc::blockchain::list_image_versions(req, headers.into(), read).scope_boxed())
        .await
}

async fn add_node_type(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::BlockchainServiceAddNodeTypeRequest>,
) -> Result<Json<api::BlockchainServiceAddNodeTypeResponse>, super::Error> {
    ctx.write(|write| grpc::blockchain::add_node_type(req, headers.into(), write).scope_boxed())
        .await
}

async fn add_version(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::BlockchainServiceAddVersionRequest>,
) -> Result<Json<api::BlockchainServiceAddVersionResponse>, super::Error> {
    ctx.write(|write| grpc::blockchain::add_version(req, headers.into(), write).scope_boxed())
        .await
}

async fn pricing(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::BlockchainServicePricingRequest>,
) -> Result<Json<api::BlockchainServicePricingResponse>, super::Error> {
    ctx.read(|read| grpc::blockchain::pricing(req, headers.into(), read).scope_boxed())
        .await
}
