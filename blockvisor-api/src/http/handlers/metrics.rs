use std::sync::Arc;

use axum::extract::State;
use axum::routing::{self, Router};
use axum::Json;
use diesel_async::scoped_futures::ScopedFutureExt;

use crate::config::Context;
use crate::database::Transaction;
use crate::grpc::metrics::RespOrError;
use crate::grpc::{self, api, Status};

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/node", routing::post(node))
        .route("/host", routing::post(host))
        .with_state(context)
}

async fn node(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::MetricsServiceNodeRequest>,
) -> Result<Json<api::MetricsServiceNodeResponse>, super::Error> {
    let outcome: Result<Json<RespOrError<api::MetricsServiceNodeResponse>>, super::Error> = ctx
        .write(|write| grpc::metrics::node(req, headers.into(), write).scope_boxed())
        .await;
    match outcome?.0 {
        RespOrError::Resp(resp) => Ok(Json(resp)),
        RespOrError::Error(error) => Err(Status::from(error).into()),
    }
}

async fn host(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::MetricsServiceHostRequest>,
) -> Result<Json<api::MetricsServiceHostResponse>, super::Error> {
    let outcome: Result<Json<RespOrError<api::MetricsServiceHostResponse>>, super::Error> = ctx
        .write(|write| grpc::metrics::host(req, headers.into(), write).scope_boxed())
        .await;
    match outcome?.0 {
        RespOrError::Resp(resp) => Ok(Json(resp)),
        RespOrError::Error(error) => Err(Status::from(error).into()),
    }
}
