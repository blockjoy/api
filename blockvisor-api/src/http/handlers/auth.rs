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
        .route("/login", routing::post(login))
        .route("/confirm", routing::post(confirm))
        .route("/refresh", routing::post(refresh))
        .route("/reset_password", routing::post(reset_password))
        .route("/password", routing::put(update_password))
        .route("/ui_password", routing::put(update_ui_password))
        .route("/permissions", routing::get(list_permissions))
        .with_state(context)
}

async fn login(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::AuthServiceLoginRequest>,
) -> Result<Json<api::AuthServiceLoginResponse>, super::Error> {
    ctx.write(|write| grpc::auth::login(req, headers.into(), write).scope_boxed())
        .await
}

async fn confirm(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::AuthServiceConfirmRequest>,
) -> Result<Json<api::AuthServiceConfirmResponse>, super::Error> {
    ctx.write(|write| grpc::auth::confirm(req, headers.into(), write).scope_boxed())
        .await
}

async fn refresh(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::AuthServiceRefreshRequest>,
) -> Result<Json<api::AuthServiceRefreshResponse>, super::Error> {
    ctx.write(|write| grpc::auth::refresh(req, headers.into(), write).scope_boxed())
        .await
}

async fn reset_password(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::AuthServiceResetPasswordRequest>,
) -> Result<Json<api::AuthServiceResetPasswordResponse>, super::Error> {
    ctx.write(|write| grpc::auth::reset_password(req, headers.into(), write).scope_boxed())
        .await
}

async fn update_password(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::AuthServiceUpdatePasswordRequest>,
) -> Result<Json<api::AuthServiceUpdatePasswordResponse>, super::Error> {
    ctx.write(|write| grpc::auth::update_password(req, headers.into(), write).scope_boxed())
        .await
}

async fn update_ui_password(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::AuthServiceUpdateUiPasswordRequest>,
) -> Result<Json<api::AuthServiceUpdateUiPasswordResponse>, super::Error> {
    ctx.write(|write| grpc::auth::update_ui_password(req, headers.into(), write).scope_boxed())
        .await
}

async fn list_permissions(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Query(req): Query<api::AuthServiceListPermissionsRequest>,
) -> Result<Json<api::AuthServiceListPermissionsResponse>, super::Error> {
    ctx.write(|write| grpc::auth::list_permissions(req, headers.into(), write).scope_boxed())
        .await
}
