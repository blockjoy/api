use blockvisor_api::auth::{self, JwtToken};
use blockvisor_api::grpc::api::{self, authentication_client};
use blockvisor_api::models;
use std::collections::HashMap;

type Service = authentication_client::AuthenticationClient<super::Channel>;

#[tokio::test]
async fn responds_ok_with_valid_credentials_for_login() {
    let tester = super::Tester::new().await;
    let mut conn = tester.conn().await;
    // confirm admin user, otherwise login would fail
    models::User::confirm(tester.admin_user().await.id, &mut conn)
        .await
        .unwrap();
    let req = api::LoginUserRequest {
        email: "admin@here.com".to_string(),
        password: "abc12345".to_string(),
    };
    tester.send(Service::login, req).await.unwrap();
}

#[tokio::test]
async fn responds_unauthenticated_with_valid_credentials_for_unconfirmed_user_login() {
    let tester = super::Tester::new().await;
    let req = api::LoginUserRequest {
        email: "admin@here.com".to_string(),
        password: "abc12345".to_string(),
    };

    let status = tester.send(Service::login, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_error_with_invalid_credentials_for_login() {
    let tester = super::Tester::new().await;
    let bogus = api::LoginUserRequest {
        email: "foo@bar.com".to_string(),
        password: "eafe12345".to_string(),
    };
    let status = tester.send(Service::login, bogus).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_ok_with_valid_credentials_for_confirm() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;
    let mut token_data = HashMap::<String, String>::new();
    token_data.insert("email".into(), "hugo@boss.com".into());
    let token = auth::RegistrationConfirmationToken::create_token_for(
        &user,
        auth::TokenType::RegistrationConfirmation,
        auth::TokenRole::User,
        Some(token_data),
    )
    .unwrap();
    let req = api::ConfirmRegistrationRequest {};
    tester
        .send_with(Service::confirm, req, token, super::DummyRefresh)
        .await
        .unwrap();
    let mut conn = tester.conn().await;
    let confirmed = models::User::is_confirmed(tester.admin_user().await.id, &mut conn)
        .await
        .unwrap();
    assert!(confirmed);
}

#[tokio::test]
async fn responds_ok_with_valid_credentials_for_refresh() {
    let tester = super::Tester::new().await;
    let req = api::RefreshTokenRequest {};
    let status = tester.send_admin(Service::refresh, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unimplemented);
}

#[tokio::test]
async fn responds_unauthenticated_with_invalid_credentials_for_refresh() {
    let tester = super::Tester::new().await;
    let invalid_token = base64::encode("asdf.asdfasdfasdfasdfasdf.asfasdfasdfasdfaf");
    let invalid_token = super::DummyToken(&invalid_token);
    let req = api::RefreshTokenRequest {};
    let status = tester
        .send_with(Service::refresh, req, invalid_token, super::DummyRefresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_ok_with_valid_passwords_for_update_ui_password() {
    let tester = super::Tester::new().await;
    let req = api::UpdateUiPasswordRequest {
        new_password: "hugo-boss".to_string(),
        old_password: "abc12345".to_string(),
    };
    tester
        .send_admin(Service::update_ui_password, req)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_unauthenticated_with_invalid_old_password_for_update_ui_password() {
    let tester = super::Tester::new().await;
    let req = api::UpdateUiPasswordRequest {
        new_password: "hugo-boss".to_string(),
        old_password: "some-wrong-pwd".to_string(),
    };
    let status = tester
        .send_admin(Service::update_ui_password, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}
