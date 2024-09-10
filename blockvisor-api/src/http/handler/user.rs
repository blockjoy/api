use std::sync::Arc;

use axum::extract::{Path, Query, State};
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
        .route("/", routing::post(create))
        .route("/:user_id", routing::get(get))
        .route("/", routing::get(list))
        .route("/", routing::put(update))
        .route("/:user_id", routing::delete(delete))
        .route("/:user_id/settings", routing::get(get_settings))
        .route("/:user_id/settings", routing::put(update_settings))
        .route("/:user_id/settings", routing::delete(delete_settings))
        .with_state(context)
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::UserServiceCreateRequest>,
) -> Result<Json<api::UserServiceCreateResponse>, Error> {
    ctx.write(|write| grpc::user::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn get(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((user_id,)): Path<(String,)>,
) -> Result<Json<api::UserServiceGetResponse>, Error> {
    let req = api::UserServiceGetRequest { user_id };
    ctx.read(|read| grpc::user::get(req, headers.into(), read).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::UserServiceListRequest>,
) -> Result<Json<api::UserServiceListResponse>, Error> {
    ctx.read(|read| grpc::user::list(req, headers.into(), read).scope_boxed())
        .await
}

async fn update(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::UserServiceUpdateRequest>,
) -> Result<Json<api::UserServiceUpdateResponse>, Error> {
    ctx.write(|write| grpc::user::update(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((user_id,)): Path<(String,)>,
) -> Result<Json<api::UserServiceDeleteResponse>, Error> {
    let req = api::UserServiceDeleteRequest { user_id };
    ctx.write(|write| grpc::user::delete(req, headers.into(), write).scope_boxed())
        .await
}

async fn get_settings(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((user_id,)): Path<(String,)>,
) -> Result<Json<api::UserServiceGetSettingsResponse>, Error> {
    let req = api::UserServiceGetSettingsRequest { user_id };
    ctx.read(|read| grpc::user::get_settings(req, headers.into(), read).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct UserServiceUpdateSettingsRequest {
    key: String,
    value: String,
}

async fn update_settings(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((user_id,)): Path<(String,)>,
    Json(req): Json<UserServiceUpdateSettingsRequest>,
) -> Result<Json<api::UserServiceUpdateSettingsResponse>, Error> {
    let req = api::UserServiceUpdateSettingsRequest {
        user_id,
        key: req.key,
        value: req.value.as_bytes().to_vec(),
    };
    ctx.write(|write| grpc::user::update_settings(req, headers.into(), write).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct UserServiceDeleteSettingsRequest {
    key: String,
}

async fn delete_settings(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((user_id,)): Path<(String,)>,
    Json(req): Json<UserServiceDeleteSettingsRequest>,
) -> Result<Json<api::UserServiceDeleteSettingsResponse>, Error> {
    let req = api::UserServiceDeleteSettingsRequest {
        user_id,
        key: req.key,
    };
    ctx.write(|write| grpc::user::delete_settings(req, headers.into(), write).scope_boxed())
        .await
}
