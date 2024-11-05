use std::sync::Arc;

use axum::extract::{Path, State};
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
        .route("/:user_id", routing::get(get))
        .route("/", routing::get(list))
        .route("/", routing::put(update))
        .route("/:user_id", routing::delete(delete))
        .route("/:user_id/billing", routing::get(get_billing))
        .route("/:user_id/billing", routing::put(update_billing))
        .route("/:user_id/billing", routing::delete(delete_billing))
        .route("/:user_id/settings", routing::get(get_settings))
        .route("/:user_id/settings", routing::put(update_settings))
        .route("/:user_id/settings", routing::delete(delete_settings))
        .with_state(context)
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::UserServiceCreateRequest>,
) -> Result<Json<api::UserServiceCreateResponse>, super::Error> {
    ctx.write(|write| grpc::user::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn get(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((id,)): Path<(String,)>,
) -> Result<Json<api::UserServiceGetResponse>, super::Error> {
    let req = api::UserServiceGetRequest { id };
    ctx.read(|read| grpc::user::get(req, headers.into(), read).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::UserServiceListRequest>,
) -> Result<Json<api::UserServiceListResponse>, super::Error> {
    ctx.read(|read| grpc::user::list(req, headers.into(), read).scope_boxed())
        .await
}

async fn update(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Json(req): Json<api::UserServiceUpdateRequest>,
) -> Result<Json<api::UserServiceUpdateResponse>, super::Error> {
    ctx.write(|write| grpc::user::update(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((user_id,)): Path<(String,)>,
) -> Result<Json<api::UserServiceDeleteResponse>, super::Error> {
    let req = api::UserServiceDeleteRequest { id: user_id };
    ctx.write(|write| grpc::user::delete(req, headers.into(), write).scope_boxed())
        .await
}

async fn get_billing(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((user_id,)): Path<(String,)>,
) -> Result<Json<api::UserServiceGetBillingResponse>, super::Error> {
    let req = api::UserServiceGetBillingRequest { user_id };
    ctx.read(|read| grpc::user::get_billing(req, headers.into(), read).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
struct UserServiceUpdateBillingRequest {
    billing_id: Option<String>,
}

async fn update_billing(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((user_id,)): Path<(String,)>,
    Json(req): Json<UserServiceUpdateBillingRequest>,
) -> Result<Json<api::UserServiceUpdateBillingResponse>, super::Error> {
    let req = api::UserServiceUpdateBillingRequest {
        user_id,
        billing_id: req.billing_id,
    };
    ctx.write(|write| grpc::user::update_billing(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete_billing(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((user_id,)): Path<(String,)>,
) -> Result<Json<api::UserServiceDeleteBillingResponse>, super::Error> {
    let req = api::UserServiceDeleteBillingRequest { user_id };
    ctx.write(|write| grpc::user::delete_billing(req, headers.into(), write).scope_boxed())
        .await
}

async fn get_settings(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((user_id,)): Path<(String,)>,
) -> Result<Json<api::UserServiceGetSettingsResponse>, super::Error> {
    let req = api::UserServiceGetSettingsRequest { user_id };
    ctx.read(|read| grpc::user::get_settings(req, headers.into(), read).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
struct UserServiceUpdateSettingsRequest {
    name: String,
    value: String,
}

async fn update_settings(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((user_id,)): Path<(String,)>,
    Json(req): Json<UserServiceUpdateSettingsRequest>,
) -> Result<Json<api::UserServiceUpdateSettingsResponse>, super::Error> {
    let req = api::UserServiceUpdateSettingsRequest {
        user_id,
        name: req.name,
        value: req.value.as_bytes().to_vec(),
    };
    ctx.write(|write| grpc::user::update_settings(req, headers.into(), write).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
struct UserServiceDeleteSettingsRequest {
    name: String,
}

async fn delete_settings(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((user_id,)): Path<(String,)>,
    Json(req): Json<UserServiceDeleteSettingsRequest>,
) -> Result<Json<api::UserServiceDeleteSettingsResponse>, super::Error> {
    let req = api::UserServiceDeleteSettingsRequest {
        user_id,
        name: req.name,
    };
    ctx.write(|write| grpc::user::delete_settings(req, headers.into(), write).scope_boxed())
        .await
}
