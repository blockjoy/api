use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::header::HeaderMap;
use axum::routing::{self, Router};
use axum::Json;
use diesel_async::scoped_futures::ScopedFutureExt;

use crate::config::Context;
use crate::database::Transaction;
use crate::grpc::{self, api};

use super::Error;

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/metadata", routing::get(get_download_metadata))
        .route("/chunks", routing::get(get_download_chunks))
        .route("/slots", routing::get(get_upload_slots))
        .route("/manifest", routing::put(put_download_manifest))
        .with_state(context)
}

async fn get_download_metadata(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::ArchiveServiceGetDownloadMetadataRequest>,
) -> Result<Json<api::ArchiveServiceGetDownloadMetadataResponse>, Error> {
    ctx.read(|read| grpc::archive::get_download_metadata(req, headers.into(), read).scope_boxed())
        .await
}

async fn get_download_chunks(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::ArchiveServiceGetDownloadChunksRequest>,
) -> Result<Json<api::ArchiveServiceGetDownloadChunksResponse>, Error> {
    ctx.read(|read| grpc::archive::get_download_chunks(req, headers.into(), read).scope_boxed())
        .await
}

async fn get_upload_slots(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::ArchiveServiceGetUploadSlotsRequest>,
) -> Result<Json<api::ArchiveServiceGetUploadSlotsResponse>, Error> {
    ctx.read(|read| grpc::archive::get_upload_slots(req, headers.into(), read).scope_boxed())
        .await
}

async fn put_download_manifest(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::ArchiveServicePutDownloadManifestRequest>,
) -> Result<Json<api::ArchiveServicePutDownloadManifestResponse>, Error> {
    ctx.read(|read| grpc::archive::put_download_manifest(req, headers.into(), read).scope_boxed())
        .await
}
