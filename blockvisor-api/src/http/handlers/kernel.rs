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
        .route("/retrieve", routing::get(retrieve_kernel))
        .route("/versions", routing::get(list_kernel_versions))
        .with_state(context)
}

async fn retrieve_kernel(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::KernelServiceRetrieveRequest>,
) -> Result<Json<api::KernelServiceRetrieveResponse>, super::Error> {
    ctx.read(|read| grpc::kernel::retrieve_kernel(req, headers.into(), read).scope_boxed())
        .await
}

async fn list_kernel_versions(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::KernelServiceListKernelVersionsRequest>,
) -> Result<Json<api::KernelServiceListKernelVersionsResponse>, super::Error> {
    ctx.read(|read| grpc::kernel::list_kernel_versions(req, headers.into(), read).scope_boxed())
        .await
}
