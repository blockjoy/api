#[allow(dead_code)]
mod setup;

use crate::setup::{get_admin_user, get_test_host};
use api::auth::TokenIdentifyable;
use api::grpc::blockjoy_ui::command_service_client::CommandServiceClient;
use api::grpc::blockjoy_ui::{CommandRequest as GrpcCommandRequest, RequestMeta, Uuid as GrpcUuid};
use setup::{server_and_client_stub, setup};
use std::sync::Arc;
use test_macros::*;
use tonic::transport::Channel;
use tonic::{Request, Status};
use uuid::Uuid;

macro_rules! test_response_ok {
    ($func:tt, $db: expr) => {{
        let request_meta = RequestMeta {
            id: Some(GrpcUuid::from(Uuid::new_v4())),
            token: None,
            fields: vec![],
            pagination: None,
        };
        let host = get_test_host(&$db).await;
        let user = get_admin_user(&$db).await;
        let token = user.get_token(&$db).await.unwrap();
        let inner = GrpcCommandRequest {
            meta: Some(request_meta),
            id: Some(GrpcUuid::from(host.id)),
            params: vec![],
        };
        let mut request = Request::new(inner);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", token.to_base64()).parse().unwrap(),
        );

        assert_grpc_request! { $func, request, tonic::Code::Ok, $db, CommandServiceClient<Channel> };
    }}
}

macro_rules! test_response_internal {
    ($func:tt, $db: expr) => {{
        let request_meta = RequestMeta {
            id: Some(GrpcUuid::from(Uuid::new_v4())),
            token: None,
            fields: vec![],
            pagination: None,
        };
        let host = get_test_host(&$db).await;
        let user = get_admin_user(&$db).await;
        let token = user.get_token(&$db).await.unwrap();
        let inner = GrpcCommandRequest {
            meta: Some(request_meta),
            id: Some(GrpcUuid::from(host.id)),
            params: vec![],
        };
        let mut request = Request::new(inner);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", token.to_base64()).parse().unwrap(),
        );

        assert_grpc_request! { $func, request, tonic::Code::Internal, $db, CommandServiceClient<Channel> };
    }}
}

macro_rules! test_response_not_found {
    ($func:tt, $db: expr) => {{
        let request_meta = RequestMeta {
            id: Some(GrpcUuid::from(Uuid::new_v4())),
            token: None,
            fields: vec![],
            pagination: None,
        };
        let user = get_admin_user(&$db).await;
        let token = user.get_token(&$db).await.unwrap();
        let inner = GrpcCommandRequest {
            meta: Some(request_meta),
            id: None,
            params: vec![],
        };
        let mut request = Request::new(inner);

        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", token.to_base64()).parse().unwrap(),
        );

        assert_grpc_request! { $func, request, tonic::Code::NotFound, $db, CommandServiceClient<Channel> };
    }}
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_create_node() {
    let db = Arc::new(_before_values.await);
    test_response_ok! { create_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_for_create_node() {
    let db = Arc::new(_before_values.await);
    test_response_internal! { create_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_create_node() {
    let db = Arc::new(_before_values.await);
    test_response_not_found! { create_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_delete_node() {
    let db = Arc::new(_before_values.await);
    test_response_ok! { delete_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_for_delete_node() {
    let db = Arc::new(_before_values.await);
    test_response_internal! { delete_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_delete_node() {
    let db = Arc::new(_before_values.await);
    test_response_not_found! { delete_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_start_node() {
    let db = Arc::new(_before_values.await);
    test_response_ok! { start_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_for_start_node() {
    let db = Arc::new(_before_values.await);
    test_response_internal! { start_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_start_node() {
    let db = Arc::new(_before_values.await);
    test_response_not_found! { start_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_stop_node() {
    let db = Arc::new(_before_values.await);
    test_response_ok! { stop_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_for_stop_node() {
    let db = Arc::new(_before_values.await);
    test_response_internal! { stop_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_stop_node() {
    let db = Arc::new(_before_values.await);
    test_response_not_found! { stop_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_restart_node() {
    let db = Arc::new(_before_values.await);
    test_response_ok! { restart_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_for_restart_node() {
    let db = Arc::new(_before_values.await);
    test_response_internal! { restart_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_restart_node() {
    let db = Arc::new(_before_values.await);
    test_response_not_found! { restart_node, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_create_host() {
    let db = Arc::new(_before_values.await);
    test_response_ok! { create_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_for_create_host() {
    let db = Arc::new(_before_values.await);
    test_response_internal! { create_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_create_host() {
    let db = Arc::new(_before_values.await);
    test_response_not_found! { create_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_delete_host() {
    let db = Arc::new(_before_values.await);
    test_response_ok! { delete_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_for_delete_host() {
    let db = Arc::new(_before_values.await);
    test_response_internal! { delete_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_delete_host() {
    let db = Arc::new(_before_values.await);
    test_response_not_found! { delete_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_start_host() {
    let db = Arc::new(_before_values.await);
    test_response_ok! { start_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_for_start_host() {
    let db = Arc::new(_before_values.await);
    test_response_internal! { start_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_start_host() {
    let db = Arc::new(_before_values.await);
    test_response_not_found! { start_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_stop_host() {
    let db = Arc::new(_before_values.await);
    test_response_ok! { stop_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_for_stop_host() {
    let db = Arc::new(_before_values.await);
    test_response_internal! { stop_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_stop_host() {
    let db = Arc::new(_before_values.await);
    test_response_not_found! { stop_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_restart_host() {
    let db = Arc::new(_before_values.await);
    test_response_ok! { restart_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_for_restart_host() {
    let db = Arc::new(_before_values.await);
    test_response_internal! { restart_host, db }
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_restart_host() {
    let db = Arc::new(_before_values.await);
    test_response_not_found! { restart_host, db }
}
