mod setup;

use api::auth::{self, JwtToken};
use api::grpc::blockjoy_ui;
use api::grpc::blockjoy_ui::authentication_service_client::AuthenticationServiceClient;
use api::models;
use std::collections::HashMap;
use tonic::transport::Channel;

type Service = AuthenticationServiceClient<Channel>;

#[tokio::test]
async fn responds_ok_with_valid_credentials_for_login() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let mut conn = tester.conn().await;
    // confirm admin user, otherwise login would fail
    models::User::confirm(tester.admin_user().await.id, &mut conn).await?;
    let req = blockjoy_ui::LoginUserRequest {
        meta: Some(tester.meta()),
        email: "admin@here.com".to_string(),
        password: "abc12345".to_string(),
    };
    tester.send(Service::login, req).await?;
    Ok(())
}

#[tokio::test]
async fn responds_unauthenticated_with_valid_credentials_for_unconfirmed_user_login() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::LoginUserRequest {
        meta: Some(tester.meta()),
        email: "admin@here.com".to_string(),
        password: "abc12345".to_string(),
    };

    let status = tester.send(Service::login, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_error_with_invalid_credentials_for_login() {
    let tester = setup::Tester::new().await;
    let bogus = blockjoy_ui::LoginUserRequest {
        meta: Some(tester.meta()),
        email: "foo@bar.com".to_string(),
        password: "eafe12345".to_string(),
    };
    let status = tester.send(Service::login, bogus).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_ok_with_valid_credentials_for_confirm() {
    let tester = setup::Tester::new().await;
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
    let req = blockjoy_ui::ConfirmRegistrationRequest {
        meta: Some(tester.meta()),
    };
    tester
        .send_with(Service::confirm, req, token, setup::DummyRefresh)
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
    let tester = setup::Tester::new().await;
    let token = tester.admin_token().await.0.to_base64().unwrap();
    let meta = tester.meta().with_token(token);
    let req = blockjoy_ui::RefreshTokenRequest { meta: Some(meta) };
    let status = tester.send_admin(Service::refresh, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unimplemented);
}

#[tokio::test]
async fn responds_unauthenticated_with_invalid_credentials_for_refresh() {
    let tester = setup::Tester::new().await;
    let invalid_token = base64::encode("asdf.asdfasdfasdfasdfasdf.asfasdfasdfasdfaf");
    let invalid_token = setup::DummyToken(&invalid_token);
    let token = tester.admin_token().await.0.to_base64().unwrap();
    let meta = tester.meta().with_token(token);
    let req = blockjoy_ui::RefreshTokenRequest { meta: Some(meta) };
    let status = tester
        .send_with(Service::refresh, req, invalid_token, setup::DummyRefresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_ok_with_valid_pwds_for_update_ui_pwd() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::UpdateUiPasswordRequest {
        meta: Some(tester.meta()),
        new_pwd: "hugo-boss".to_string(),
        new_pwd_confirmation: "hugo-boss".to_string(),
        old_pwd: "abc12345".to_string(),
    };
    tester
        .send_admin(Service::update_ui_password, req)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_unauthenticated_with_invalid_old_pwd_for_update_ui_pwd() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::UpdateUiPasswordRequest {
        meta: Some(tester.meta()),
        new_pwd: "hugo-boss".to_string(),
        new_pwd_confirmation: "hugo-boss".to_string(),
        old_pwd: "some-wrong-pwd".to_string(),
    };
    let status = tester
        .send_admin(Service::update_ui_password, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_invalid_argument_with_invalid_pwd_confirmation_for_update_ui_pwd() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::UpdateUiPasswordRequest {
        meta: Some(tester.meta()),
        new_pwd: "hugo-boss".to_string(),
        new_pwd_confirmation: "hugo-employee".to_string(),
        old_pwd: "abc12345".to_string(),
    };
    let status = tester
        .send_admin(Service::update_ui_password, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}
