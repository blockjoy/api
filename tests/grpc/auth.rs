use blockvisor_api::auth::claims::Claims;
use blockvisor_api::auth::endpoint::Endpoint;
use blockvisor_api::auth::token::refresh::Refresh;
use blockvisor_api::auth::token::RequestToken;
use blockvisor_api::grpc::api::{self, auth_service_client};
use blockvisor_api::models;

type Service = auth_service_client::AuthServiceClient<super::Channel>;

#[tokio::test]
async fn responds_ok_with_valid_credentials_for_login() {
    let tester = super::Tester::new().await;
    let user = tester.user().await;
    let req = api::AuthServiceLoginRequest {
        email: user.email,
        password: "abc12345".to_string(),
    };
    tester.send(Service::login, req).await.unwrap();
}

#[tokio::test]
async fn responds_forbiddenenticated_with_valid_credentials_for_unconfirmed_user_login() {
    let tester = super::Tester::new().await;
    let user = tester.unconfirmed_user().await;
    let req = api::AuthServiceLoginRequest {
        email: user.email,
        password: "abc12345".to_string(),
    };

    let status = tester.send(Service::login, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated, "{status:?}");
}

#[tokio::test]
async fn responds_error_with_invalid_credentials_for_login() {
    let tester = super::Tester::new().await;
    let bogus = api::AuthServiceLoginRequest {
        email: "foo@bar.com".to_string(),
        password: "eafe12345".to_string(),
    };
    let status = tester.send(Service::login, bogus).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_ok_with_valid_credentials_for_confirm() {
    let tester = super::Tester::new().await;
    let user = tester.unconfirmed_user().await;

    let expires = chrono::Duration::minutes(15);
    let claims = Claims::user_from_now(expires, user.id, [Endpoint::AuthConfirm]);
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let req = api::AuthServiceConfirmRequest {};
    tester.send_with(Service::confirm, req, &jwt).await.unwrap();

    let mut conn = tester.conn().await;
    let confirmed = models::User::is_confirmed(tester.unconfirmed_user().await.id, &mut conn)
        .await
        .unwrap();
    assert!(confirmed);
}

#[tokio::test]
async fn responds_ok_for_refresh() {
    let tester = super::Tester::new().await;

    let claims = tester.admin_token().await;
    let token = tester.cipher().jwt.encode(&claims).unwrap();

    let refresh = tester.admin_refresh().await;
    let encoded = tester.cipher().refresh.encode(&refresh).unwrap();

    let req = api::AuthServiceRefreshRequest {
        token: token.into(),
        refresh: Some(encoded.into()),
    };
    let resp = tester.send_admin(Service::refresh, req).await.unwrap();

    let token = match resp.token.parse().unwrap() {
        RequestToken::Bearer(token) => token,
        _ => panic!("Unexpected RequestToken type"),
    };
    let refresh = resp.refresh.into();

    tester.cipher().jwt.decode(&token).unwrap();
    tester.cipher().refresh.decode(&refresh).unwrap();
}

#[tokio::test]
async fn responds_ok_with_valid_passwords_for_update_ui_password() {
    let tester = super::Tester::new().await;
    let req = api::AuthServiceUpdateUiPasswordRequest {
        user_id: tester.user().await.id.to_string(),
        new_password: "hugo-boss".to_string(),
        old_password: "abc12345".to_string(),
    };
    tester
        .send_admin(Service::update_ui_password, req)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_forbiddenenticated_with_invalid_old_password_for_update_ui_password() {
    let tester = super::Tester::new().await;
    let req = api::AuthServiceUpdateUiPasswordRequest {
        user_id: tester.user().await.id.to_string(),
        new_password: "hugo-boss".to_string(),
        old_password: "some-wrong-pwd".to_string(),
    };
    let status = tester
        .send_admin(Service::update_ui_password, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn refresh_works() {
    let tester = super::Tester::new().await;

    let claims = tester.admin_token().await;
    let token = tester.cipher().jwt.encode(&claims).unwrap();

    let refresh = tester.admin_refresh().await;
    let encoded = tester.cipher().refresh.encode(&refresh).unwrap();

    let req = api::AuthServiceRefreshRequest {
        token: token.into(),
        refresh: Some(encoded.into()),
    };

    tester.send(Service::refresh, req).await.unwrap();
}

#[tokio::test]
async fn refresh_works_from_cookie() {
    let tester = super::Tester::new().await;

    let claims = tester.admin_token().await;
    let token = tester.cipher().jwt.encode(&claims).unwrap();

    let expires = chrono::Duration::seconds(60);
    let refresh = Refresh::from_now(expires, tester.user().await.id);
    let encoded = tester.cipher().refresh.encode(&refresh).unwrap();

    let req = api::AuthServiceRefreshRequest {
        token: token.into(),
        refresh: None,
    };
    let mut req = tonic::Request::new(req);
    req.metadata_mut()
        .insert("cookie", format!("refresh={}", *encoded).parse().unwrap());

    tester.send(Service::refresh, req).await.unwrap();
}
