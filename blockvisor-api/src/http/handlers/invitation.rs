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
        .route("/", routing::post(create))
        .route("/", routing::get(list))
        .route("/:id/accept", routing::post(accept))
        .route("/:id/decline", routing::post(decline))
        .route("/:id/revoke", routing::post(revoke))
        .with_state(context)
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::InvitationServiceCreateRequest>,
) -> Result<Json<api::InvitationServiceCreateResponse>, super::Error> {
    ctx.write(|write| grpc::invitation::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::InvitationServiceListRequest>,
) -> Result<Json<api::InvitationServiceListResponse>, super::Error> {
    ctx.read(|read| grpc::invitation::list(req, headers.into(), read).scope_boxed())
        .await
}

async fn accept(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((invitation_id,)): Path<(String,)>,
) -> Result<Json<api::InvitationServiceAcceptResponse>, super::Error> {
    let req = api::InvitationServiceAcceptRequest { invitation_id };
    ctx.write(|write| grpc::invitation::accept(req, headers.into(), write).scope_boxed())
        .await
}

async fn decline(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((invitation_id,)): Path<(String,)>,
) -> Result<Json<api::InvitationServiceDeclineResponse>, super::Error> {
    let req = api::InvitationServiceDeclineRequest { invitation_id };
    ctx.write(|write| grpc::invitation::decline(req, headers.into(), write).scope_boxed())
        .await
}

async fn revoke(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((invitation_id,)): Path<(String,)>,
) -> Result<Json<api::InvitationServiceRevokeResponse>, super::Error> {
    let req = api::InvitationServiceRevokeRequest { invitation_id };
    ctx.write(|write| grpc::invitation::revoke(req, headers.into(), write).scope_boxed())
        .await
}
