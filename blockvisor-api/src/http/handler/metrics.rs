use std::sync::Arc;

use axum::extract::State;
use axum::http::header::HeaderMap;
use axum::routing::{self, Router};
use axum::Json;
use diesel_async::scoped_futures::ScopedFutureExt;

use crate::config::Context;
use crate::database::Transaction;
use crate::grpc::metrics::AfterCommit;
use crate::grpc::{self, api, Status};

use super::Error;

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/host", routing::post(host))
        .route("/node", routing::post(node))
        .with_state(context)
}

async fn host(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::MetricsServiceHostRequest>,
) -> Result<Json<api::MetricsServiceHostResponse>, Error> {
    ctx.write(|write| grpc::metrics::host(req, headers.into(), write).scope_boxed())
        .await
}

async fn node(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::MetricsServiceNodeRequest>,
) -> Result<Json<api::MetricsServiceNodeResponse>, Error> {
    let outcome: Result<Json<AfterCommit<api::MetricsServiceNodeResponse>>, Error> = ctx
        .write(|write| grpc::metrics::node(req, headers.into(), write).scope_boxed())
        .await;
    match outcome?.0 {
        AfterCommit::Ok(resp) => Ok(Json(resp)),
        AfterCommit::Err(err) => Err(Status::from(err).into()),
    }
}
