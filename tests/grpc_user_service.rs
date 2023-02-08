mod setup;

use api::grpc::blockjoy_ui::{self, user_service_client, GetUserRequest};
use api::models;
use tonic::transport;

type Service = user_service_client::UserServiceClient<transport::Channel>;

#[tokio::test]
async fn responds_ok_with_valid_token_for_get() {
    let tester = setup::Tester::new().await;
    let req = GetUserRequest {
        meta: Some(tester.meta()),
    };
    tester.send_admin(Service::get, req).await.unwrap();
}

/// THOMAS: why should this test fail? It should be allowed for this user to delete themselves
/// right?
#[tokio::test]
#[ignore]
async fn responds_not_found_with_valid_token_for_delete() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::DeleteUserRequest {
        meta: Some(tester.meta()),
    };
    let status = tester.send_admin(Service::delete, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

/// THOMAS: Why do we create a node before having the user delete themselves?
#[tokio::test]
async fn responds_ok_with_valid_token_for_delete() {
    let tester = setup::Tester::new().await;
    // create a node
    let blockchain = tester.blockchain().await;
    let user = tester.admin_user().await;
    let org = tester.org_for(&user).await;
    let mut req = models::NodeCreateRequest {
        org_id: org.id,
        blockchain_id: blockchain.id,
        node_type: sqlx::types::Json(models::NodeProperties::special_type(
            models::NodeTypeKey::Validator,
        )),
        chain_status: models::NodeChainStatus::Unknown,
        sync_status: models::NodeSyncStatus::Syncing,
        container_status: models::ContainerStatus::Installing,
        address: None,
        wallet_address: None,
        block_height: None,
        groups: None,
        node_data: None,
        ip_addr: None,
        ip_gateway: Some("192.168.0.1".into()),
        name: None,
        version: Some("3.3.0".into()),
        staking_status: None,
        self_update: false,
        vcpu_count: 0,
        mem_size_mb: 0,
        disk_size_gb: 0,
        host_name: "some host".to_string(),
        network: "some network".to_string(),
    };
    let mut tx = tester.begin().await;
    models::Node::create(&mut req, &mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let req = blockjoy_ui::DeleteUserRequest {
        meta: Some(tester.meta()),
    };

    tester.send_admin(Service::delete, req).await.unwrap();
}

#[tokio::test]
async fn responds_unauthenticated_without_valid_token_for_get() {
    let tester = setup::Tester::new().await;
    let token = base64::encode("some-invalid-token");
    let token = setup::DummyToken(&token);
    let req = GetUserRequest {
        meta: Some(tester.meta()),
    };
    let status = tester
        .send_with(Service::get, req, token, setup::DummyRefresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_ok_without_token_for_create() {
    let tester = setup::Tester::new().await;
    let user = blockjoy_ui::User {
        id: None,
        email: Some("hugo@boss.com".to_string()),
        first_name: Some("Hugo".to_string()),
        last_name: Some("The Bossman".to_string()),
        created_at: None,
        updated_at: None,
    };
    let req = blockjoy_ui::CreateUserRequest {
        meta: Some(tester.meta()),
        password: "abcde12345".to_string(),
        password_confirmation: "abcde12345".to_string(),
        user: Some(user),
    };
    tester.send(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_with_existing_email_for_create() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let grpc_user = blockjoy_ui::User {
        email: Some(user.email),
        first_name: Some(user.first_name),
        last_name: Some(user.last_name),
        ..Default::default()
    };
    let req = blockjoy_ui::CreateUserRequest {
        meta: Some(tester.meta()),
        password: "abcde12345".to_string(),
        password_confirmation: "abcde12345".to_string(),
        user: Some(grpc_user),
    };
    let status = tester.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_error_with_different_pwds_for_create() {
    let tester = setup::Tester::new().await;
    let user = blockjoy_ui::User {
        id: None,
        email: Some("hugo@boss.com".to_string()),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
        created_at: None,
        updated_at: None,
    };
    let req = blockjoy_ui::CreateUserRequest {
        meta: Some(tester.meta()),
        password: "abcde12345".to_string(),
        password_confirmation: "54321edcba".to_string(),
        user: Some(user),
    };
    let status = tester.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_permission_denied_with_diff_users_for_update() {
    let tester = setup::Tester::new().await;
    let grpc_user = blockjoy_ui::User {
        id: Some(uuid::Uuid::new_v4().to_string()),
        email: Some("hugo@boss.com".to_string()),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
        created_at: None,
        updated_at: None,
    };
    let req = blockjoy_ui::UpdateUserRequest {
        meta: Some(tester.meta()),
        user: Some(grpc_user),
    };
    let status = tester.send_admin(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_ok_with_equal_users_for_update() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let grpc_user = blockjoy_ui::User {
        id: Some(user.id.to_string()),
        email: None,
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
        created_at: None,
        updated_at: None,
    };
    let req = blockjoy_ui::UpdateUserRequest {
        meta: Some(tester.meta()),
        user: Some(grpc_user),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}
