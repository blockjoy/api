use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::header::HeaderMap;
use axum::routing::{self, Router};
use axum::Json;
use diesel_async::scoped_futures::ScopedFutureExt;

use crate::config::Context;
use crate::database::Transaction;
use crate::grpc::{self, api, common};

use super::Error;

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/", routing::post(create))
        .route("/:id", routing::get(get))
        .route("/", routing::get(list))
        .route("/:id", routing::put(update))
        .route("/:id", routing::delete(delete))
        .route("/:id/member", routing::delete(remove_member))
        .route("/:id/provision-token", routing::get(get_provision_token))
        .route("/:id/provision-token", routing::post(reset_provision_token))
        .route("/:id/init-card", routing::post(init_card))
        .route("/:id/payment-methods", routing::get(list_payment_methods))
        .route("/:id/billing-details", routing::get(billing_details))
        .route("/:id/address", routing::get(get_address))
        .route("/:id/address", routing::post(set_address))
        .route("/:id/address", routing::delete(delete_address))
        .route("/:id/invoices", routing::get(get_invoices))
        .with_state(context)
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(req): Json<api::OrgServiceCreateRequest>,
) -> Result<Json<api::OrgServiceCreateResponse>, Error> {
    ctx.write(|write| grpc::org::create(req, headers.into(), write).scope_boxed())
        .await
}

async fn get(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
) -> Result<Json<api::OrgServiceGetResponse>, Error> {
    let req = api::OrgServiceGetRequest { org_id };
    ctx.read(|read| grpc::org::get(req, headers.into(), read).scope_boxed())
        .await
}

async fn list(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Query(req): Query<api::OrgServiceListRequest>,
) -> Result<Json<api::OrgServiceListResponse>, Error> {
    ctx.read(|read| grpc::org::list(req, headers.into(), read).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OrgServiceUpdateRequest {
    name: Option<String>,
}

async fn update(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
    Json(req): Json<OrgServiceUpdateRequest>,
) -> Result<Json<api::OrgServiceUpdateResponse>, Error> {
    let req = api::OrgServiceUpdateRequest {
        org_id,
        name: req.name,
    };
    ctx.write(|write| grpc::org::update(req, headers.into(), write).scope_boxed())
        .await
}

async fn delete(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
) -> Result<Json<api::OrgServiceDeleteResponse>, Error> {
    let req = api::OrgServiceDeleteRequest { org_id };
    ctx.write(|write| grpc::org::delete(req, headers.into(), write).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OrgServiceRemoveMemberRequest {
    user_id: String,
}

async fn remove_member(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
    Json(req): Json<OrgServiceRemoveMemberRequest>,
) -> Result<Json<api::OrgServiceRemoveMemberResponse>, Error> {
    let req = api::OrgServiceRemoveMemberRequest {
        user_id: req.user_id,
        org_id,
    };
    ctx.write(|write| grpc::org::remove_member(req, headers.into(), write).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OrgServiceGetProvisionTokenRequest {
    user_id: String,
}

async fn get_provision_token(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
    Query(req): Query<OrgServiceGetProvisionTokenRequest>,
) -> Result<Json<api::OrgServiceGetProvisionTokenResponse>, Error> {
    let req = api::OrgServiceGetProvisionTokenRequest {
        user_id: req.user_id,
        org_id,
    };
    ctx.read(|read| grpc::org::get_provision_token(req, headers.into(), read).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OrgServiceResetProvisionTokenRequest {
    user_id: String,
}

async fn reset_provision_token(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
    Json(req): Json<OrgServiceResetProvisionTokenRequest>,
) -> Result<Json<api::OrgServiceResetProvisionTokenResponse>, Error> {
    let req = api::OrgServiceResetProvisionTokenRequest {
        user_id: req.user_id,
        org_id,
    };
    ctx.write(|write| grpc::org::reset_provision_token(req, headers.into(), write).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OrgServiceInitCardRequest {
    user_id: String,
}

async fn init_card(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
    Json(req): Json<OrgServiceInitCardRequest>,
) -> Result<Json<api::OrgServiceInitCardResponse>, Error> {
    let req = api::OrgServiceInitCardRequest {
        user_id: req.user_id,
        org_id,
    };
    ctx.write(|write| grpc::org::init_card(req, headers.into(), write).scope_boxed())
        .await
}

async fn list_payment_methods(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
) -> Result<Json<api::OrgServiceListPaymentMethodsResponse>, Error> {
    let req = api::OrgServiceListPaymentMethodsRequest { org_id };
    ctx.read(|read| grpc::org::list_payment_methods(req, headers.into(), read).scope_boxed())
        .await
}

async fn billing_details(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
) -> Result<Json<api::OrgServiceBillingDetailsResponse>, Error> {
    let req = api::OrgServiceBillingDetailsRequest { org_id };
    ctx.read(|read| grpc::org::billing_details(req, headers.into(), read).scope_boxed())
        .await
}

async fn get_address(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
) -> Result<Json<api::OrgServiceGetAddressResponse>, Error> {
    let req = api::OrgServiceGetAddressRequest { org_id };
    ctx.read(|read| grpc::org::get_address(req, headers.into(), read).scope_boxed())
        .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OrgServiceSetAddressRequest {
    address: common::Address,
}

async fn set_address(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    Path((org_id,)): Path<(String,)>,
    Json(req): Json<OrgServiceSetAddressRequest>,
) -> Result<Json<api::OrgServiceSetAddressResponse>, super::Error> {
    let req = api::OrgServiceSetAddressRequest {
        org_id,
        address: Some(req.address),
    };
    ctx.read(|read| grpc::org::set_address(req, headers.into(), read).scope_boxed())
        .await
}

async fn delete_address(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
) -> Result<Json<api::OrgServiceDeleteAddressResponse>, super::Error> {
    let req = api::OrgServiceDeleteAddressRequest { org_id };
    ctx.read(|read| grpc::org::delete_address(req, headers.into(), read).scope_boxed())
        .await
}

async fn get_invoices(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Path((org_id,)): Path<(String,)>,
) -> Result<Json<api::OrgServiceGetInvoicesResponse>, super::Error> {
    let req = api::OrgServiceGetInvoicesRequest { org_id };
    ctx.read(|read| grpc::org::get_invoices(req, headers.into(), read).scope_boxed())
        .await
}
