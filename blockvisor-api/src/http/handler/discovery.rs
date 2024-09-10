use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::{self, Router};
use diesel_async::scoped_futures::ScopedFutureExt;

use crate::config::Context;
use crate::database::Transaction;
use crate::grpc::{self, api};

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/", routing::get(services))
        .with_state(context)
}

async fn services(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::DiscoveryServiceServicesRequest>,
) -> Result<axum::Json<api::DiscoveryServiceServicesResponse>, super::Error> {
    ctx.read(|read| grpc::discovery::services(req, headers.into(), read).scope_boxed())
        .await
}
