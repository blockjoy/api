use std::sync::Arc;

use axum::extract::{Query, State};
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
        .route("/", routing::get(retrieve))
        .route("/versions", routing::get(list_bundle_versions))
        .route("/", routing::delete(delete))
        .with_state(context)
}

async fn retrieve(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::BundleServiceRetrieveRequest>,
) -> Result<Json<api::BundleServiceRetrieveResponse>, super::Error> {
    ctx.read(|read| grpc::bundle::retrieve(req, headers.into(), read).scope_boxed())
        .await
}

async fn list_bundle_versions(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::BundleServiceListBundleVersionsRequest>,
) -> Result<Json<api::BundleServiceListBundleVersionsResponse>, super::Error> {
    ctx.read(|read| grpc::bundle::list_bundle_versions(req, headers.into(), read).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::BundleServiceDeleteRequest>,
) -> Result<Json<api::BundleServiceDeleteResponse>, super::Error> {
    ctx.read(|read| grpc::bundle::delete(req, headers.into(), read).scope_boxed())
        .await
}
