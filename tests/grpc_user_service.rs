#[allow(dead_code)]
mod setup;

use crate::setup::setup;
use api::auth::TokenIdentifyable;
use api::grpc::blockjoy_ui::user_service_client::UserServiceClient;
use api::grpc::blockjoy_ui::{
    CreateUserRequest, GetUserRequest, RequestMeta, UpdateUserRequest, User as GrpcUser,
};
use base64::encode;
use std::sync::Arc;
use test_macros::before;
use tonic::{transport::Channel, Request, Status};
use uuid::Uuid;

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_with_valid_token_for_get() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(Uuid::new_v4().to_string()),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = db.admin_user().await;
    let token = user.get_token(&db.pool).await.unwrap();
    let inner = GetUserRequest {
        meta: Some(request_meta),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { get, request, tonic::Code::Ok, db, UserServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_unauthenticated_without_valid_token_for_get() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(Uuid::new_v4().to_string()),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let token = encode("some-invalid-token");
    let inner = GetUserRequest {
        meta: Some(request_meta),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token).parse().unwrap(),
    );

    assert_grpc_request! { get, request, tonic::Code::Unauthenticated, db, UserServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_without_token_for_create() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(Uuid::new_v4().to_string()),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let grpc_user = GrpcUser {
        id: None,
        email: Some("hugo@boss.com".to_string()),
        first_name: Some("Hugo".to_string()),
        last_name: Some("The Bossman".to_string()),
        created_at: None,
        updated_at: None,
    };
    let inner = CreateUserRequest {
        meta: Some(request_meta),
        password: "abcde12345".to_string(),
        password_confirmation: "abcde12345".to_string(),
        user: Some(grpc_user),
    };
    let request = Request::new(inner);

    assert_grpc_request! { create, request, tonic::Code::Ok, db, UserServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_error_with_existing_email_for_create() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(Uuid::new_v4().to_string()),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = db.admin_user().await;
    let token = user.get_token(&db.pool).await.unwrap();
    let grpc_user = GrpcUser {
        id: None,
        email: Some(user.email),
        first_name: Some(user.first_name),
        last_name: Some(user.last_name),
        created_at: None,
        updated_at: None,
    };
    let inner = CreateUserRequest {
        meta: Some(request_meta),
        password: "abcde12345".to_string(),
        password_confirmation: "abcde12345".to_string(),
        user: Some(grpc_user),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { create, request, tonic::Code::InvalidArgument, db, UserServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_error_with_different_pwds_for_create() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(Uuid::new_v4().to_string()),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = db.admin_user().await;
    let token = user.get_token(&db.pool).await.unwrap();
    let grpc_user = GrpcUser {
        id: None,
        email: Some("hugo@boss.com".to_string()),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
        created_at: None,
        updated_at: None,
    };
    let inner = CreateUserRequest {
        meta: Some(request_meta),
        password: "abcde12345".to_string(),
        password_confirmation: "54321edcba".to_string(),
        user: Some(grpc_user),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { create, request, tonic::Code::InvalidArgument, db, UserServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_permission_denied_with_diff_users_for_update() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(Uuid::new_v4().to_string()),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = db.admin_user().await;
    let token = user.get_token(&db.pool).await.unwrap();
    let grpc_user = GrpcUser {
        id: Some(Uuid::new_v4().to_string()),
        email: Some("hugo@boss.com".to_string()),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
        created_at: None,
        updated_at: None,
    };
    let inner = UpdateUserRequest {
        meta: Some(request_meta),
        user: Some(grpc_user),
    };

    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { update, request, tonic::Code::PermissionDenied, db, UserServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_with_equal_users_for_update() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(Uuid::new_v4().to_string()),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = db.admin_user().await;
    let token = user.get_token(&db.pool).await.unwrap();
    let grpc_user = GrpcUser {
        id: Some(user.id.to_string()),
        email: None,
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
        created_at: None,
        updated_at: None,
    };
    let inner = UpdateUserRequest {
        meta: Some(request_meta),
        user: Some(grpc_user),
    };

    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { update, request, tonic::Code::Ok, db, UserServiceClient<Channel> };
}
