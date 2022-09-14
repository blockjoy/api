#[allow(dead_code)]
mod setup;

use crate::setup::{get_admin_user, get_blockchain, get_test_host};
use api::auth::TokenIdentifyable;
use api::grpc::blockjoy_ui::node_service_client::NodeServiceClient;
use api::grpc::blockjoy_ui::{
    node, CreateNodeRequest, GetNodeRequest, Node as GrpcNode, RequestMeta, UpdateNodeRequest,
    Uuid as GrpcUuid,
};
use api::models::{
    ContainerStatus, Node, NodeChainStatus, NodeCreateRequest, NodeSyncStatus, NodeType, Org,
};
use setup::{server_and_client_stub, setup};
use std::sync::Arc;
use test_macros::*;
use tonic::transport::Channel;
use tonic::{Request, Status};
use uuid::Uuid;

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_without_any_for_get() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = get_admin_user(&db).await;
    let token = user.get_token(&db).await.unwrap();
    let inner = GetNodeRequest {
        meta: Some(request_meta),
        id: None,
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { get, request, tonic::Code::NotFound, db, NodeServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_with_id_for_get() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let blockchain = get_blockchain(&db).await;
    let host = get_test_host(&db).await;
    let user = get_admin_user(&db).await;
    let org_id = Org::find_all_by_user(user.id, &db)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id;
    let req = NodeCreateRequest {
        host_id: host.id,
        org_id,
        blockchain_id: blockchain.id,
        node_type: NodeType::Validator,
        chain_status: NodeChainStatus::Unknown,
        sync_status: NodeSyncStatus::Syncing,
        container_status: ContainerStatus::Installing,
        address: None,
        wallet_address: None,
        block_height: None,
        groups: None,
        node_data: None,
        ip_addr: None,
        name: None,
        version: None,
        staking_status: None,
    };
    let node = Node::create(&req, &db).await.unwrap();
    let user = get_admin_user(&db).await;
    let token = user.get_token(&db).await.unwrap();
    let inner = GetNodeRequest {
        meta: Some(request_meta),
        id: Some(GrpcUuid::from(node.id)),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { get, request, tonic::Code::Ok, db, NodeServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_with_valid_data_for_create() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let blockchain = get_blockchain(&db).await;
    let host = get_test_host(&db).await;
    let user = get_admin_user(&db).await;
    let org_id = Org::find_all_by_user(user.id, &db)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id;
    let node = GrpcNode {
        id: None,
        host_id: Some(GrpcUuid::from(host.id)),
        org_id: Some(GrpcUuid::from(org_id)),
        blockchain_id: Some(GrpcUuid::from(blockchain.id)),
        name: None,
        status: Some(node::NodeStatus::UndefinedApplicationStatus as i32),
        address: None,
        r#type: Some(node::NodeType::Api as i32),
        version: None,
        wallet_address: None,
        block_height: None,
        node_data: None,
        ip: None,
        created_at: None,
        updated_at: None,
        groups: vec![],
    };
    let token = user.get_token(&db).await.unwrap();
    let inner = CreateNodeRequest {
        meta: Some(request_meta),
        node: Some(node),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { create, request, tonic::Code::Ok, db, NodeServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_with_invalid_data_for_create() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let node = GrpcNode {
        id: None,
        host_id: None,
        org_id: None,
        blockchain_id: None,
        name: None,
        status: Some(node::NodeStatus::UndefinedApplicationStatus as i32),
        address: None,
        r#type: Some(node::NodeType::Api as i32),
        version: None,
        wallet_address: None,
        block_height: None,
        node_data: None,
        ip: None,
        created_at: None,
        updated_at: None,
        groups: vec![],
    };
    let user = get_admin_user(&db).await;
    let token = user.get_token(&db).await.unwrap();
    let inner = CreateNodeRequest {
        meta: Some(request_meta),
        node: Some(node),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { create, request, tonic::Code::Internal, db, NodeServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_with_valid_data_for_update() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let blockchain = get_blockchain(&db).await;
    let host = get_test_host(&db).await;
    let user = get_admin_user(&db).await;
    let org_id = Org::find_all_by_user(user.id, &db)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id;
    let req = NodeCreateRequest {
        host_id: host.id,
        org_id,
        blockchain_id: blockchain.id,
        node_type: NodeType::Validator,
        chain_status: NodeChainStatus::Unknown,
        sync_status: NodeSyncStatus::Syncing,
        container_status: ContainerStatus::Installing,
        address: None,
        wallet_address: None,
        block_height: None,
        groups: None,
        node_data: None,
        ip_addr: None,
        name: None,
        version: None,
        staking_status: None,
    };
    let db_node = Node::create(&req, &db).await.unwrap();
    let node = GrpcNode {
        id: Some(GrpcUuid::from(db_node.id)),
        name: Some("stri-bu".to_string()),
        ..Default::default()
    };
    let token = user.get_token(&db).await.unwrap();
    let inner = UpdateNodeRequest {
        meta: Some(request_meta),
        node: Some(node),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { update, request, tonic::Code::Ok, db, NodeServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_internal_with_invalid_data_for_update() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let blockchain = get_blockchain(&db).await;
    let host = get_test_host(&db).await;
    let user = get_admin_user(&db).await;
    let org_id = Org::find_all_by_user(user.id, &db)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id;
    let req = NodeCreateRequest {
        host_id: host.id,
        org_id,
        blockchain_id: blockchain.id,
        node_type: NodeType::Validator,
        chain_status: NodeChainStatus::Unknown,
        sync_status: NodeSyncStatus::Syncing,
        container_status: ContainerStatus::Installing,
        address: None,
        wallet_address: None,
        block_height: None,
        groups: None,
        node_data: None,
        ip_addr: None,
        name: None,
        version: None,
        staking_status: None,
    };
    let db_node = Node::create(&req, &db).await.unwrap();
    let node = GrpcNode {
        id: Some(GrpcUuid::from(db_node.id)),
        name: Some("stri-bu".to_string()),
        // This should cause an error
        blockchain_id: None,
        ..Default::default()
    };
    let token = user.get_token(&db).await.unwrap();
    let inner = UpdateNodeRequest {
        meta: Some(request_meta),
        node: Some(node),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { update, request, tonic::Code::Internal, db, NodeServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_with_invalid_id_for_update() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let user = get_admin_user(&db).await;
    let node = GrpcNode {
        // This should cause an error
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        ..Default::default()
    };
    let token = user.get_token(&db).await.unwrap();
    let inner = UpdateNodeRequest {
        meta: Some(request_meta),
        node: Some(node),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { update, request, tonic::Code::NotFound, db, NodeServiceClient<Channel> };
}
