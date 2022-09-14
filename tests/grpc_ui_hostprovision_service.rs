#[allow(dead_code)]
mod setup;

use crate::setup::get_admin_user;
use api::auth::TokenIdentifyable;
use api::grpc::blockjoy_ui::host_provision_service_client::HostProvisionServiceClient;
use api::grpc::blockjoy_ui::{
    CreateHostProvisionRequest, GetHostProvisionRequest, HostProvision as GrpcHostProvision,
    RequestMeta, Uuid as GrpcUuid,
};
use api::models::{HostProvision, HostProvisionRequest};
use setup::{server_and_client_stub, setup};
use sqlx::postgres::PgRow;
use sqlx::Row;
use std::sync::Arc;
use test_macros::*;
use tonic::transport::Channel;
use tonic::{Request, Status};
use uuid::Uuid;

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_without_valid_id_for_get() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = get_admin_user(&db).await;
    let token = user.get_token(&db).await.unwrap();
    let inner = GetHostProvisionRequest {
        meta: Some(request_meta),
        id: "foo-bar1".to_string(),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { get, request, tonic::Code::NotFound, db, HostProvisionServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_with_valid_id_for_get() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = get_admin_user(&db).await;
    let mut tx = db.begin().await.unwrap();
    let org_id = sqlx::query("select org_id from orgs_users where user_id = $1 limit 1")
        .bind(user.id)
        .fetch_one(&mut tx)
        .await
        .map(PgRow::from)
        .unwrap();
    tx.commit().await.unwrap();

    let token = user.get_token(&db).await.unwrap();
    let req = HostProvisionRequest {
        org_id: org_id.get::<Uuid, usize>(0),
        nodes: None,
    };
    let provision = HostProvision::create(req, &db).await.unwrap();

    let inner = GetHostProvisionRequest {
        meta: Some(request_meta),
        id: provision.id,
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { get, request, tonic::Code::Ok, db, HostProvisionServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_error_with_invalid_provision_for_create() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = get_admin_user(&db).await;
    let token = user.get_token(&db).await.unwrap();
    let inner = CreateHostProvisionRequest {
        meta: Some(request_meta),
        host_provision: None,
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { create, request, tonic::Code::Unknown, db, HostProvisionServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_with_valid_provision_for_create() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = get_admin_user(&db).await;
    let mut tx = db.begin().await.unwrap();
    let org_id = sqlx::query("select org_id from orgs_users where user_id = $1 limit 1")
        .bind(user.id)
        .fetch_one(&mut tx)
        .await
        .map(PgRow::from)
        .unwrap();
    tx.commit().await.unwrap();

    let token = user.get_token(&db).await.unwrap();
    let provision = GrpcHostProvision {
        org_id: Some(GrpcUuid::from(org_id.get::<Uuid, usize>(0))),
        ..Default::default()
    };
    let inner = CreateHostProvisionRequest {
        meta: Some(request_meta),
        host_provision: Some(provision),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { create, request, tonic::Code::Ok, db, HostProvisionServiceClient<Channel> };
}
