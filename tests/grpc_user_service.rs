mod setup;

use api::grpc::blockjoy_ui::{self, user_service_client, GetUserRequest};
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

#[tokio::test]
async fn responds_ok_with_valid_token_for_delete() {
    let tester = setup::Tester::new().await;
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
    let req = blockjoy_ui::CreateUserRequest {
        meta: Some(tester.meta()),
        email: "hugo@boss.com".to_string(),
        first_name: "Hugo".to_string(),
        last_name: "The Bossman".to_string(),
        password: "abcde12345".to_string(),
        password_confirmation: "abcde12345".to_string(),
    };
    tester.send(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_with_existing_email_for_create() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let req = blockjoy_ui::CreateUserRequest {
        meta: Some(tester.meta()),
        email: user.email,
        first_name: user.first_name,
        last_name: user.last_name,
        password: "abcde12345".to_string(),
        password_confirmation: "abcde12345".to_string(),
    };
    let status = tester.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_error_with_different_pwds_for_create() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::CreateUserRequest {
        meta: Some(tester.meta()),
        email: "hugo@boss.com".to_string(),
        first_name: "Hugo".to_string(),
        last_name: "Boss".to_string(),
        password: "abcde12345".to_string(),
        password_confirmation: "54321edcba".to_string(),
    };
    let status = tester.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_permission_denied_with_diff_users_for_update() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::UpdateUserRequest {
        meta: Some(tester.meta()),
        id: uuid::Uuid::new_v4().to_string(),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
    };
    let status = tester.send_admin(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_ok_with_equal_users_for_update() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let req = blockjoy_ui::UpdateUserRequest {
        meta: Some(tester.meta()),
        id: user.id.to_string(),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}
