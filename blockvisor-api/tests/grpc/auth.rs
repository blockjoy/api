use tonic::transport::Channel;

use blockvisor_api::auth::claims::Claims;
use blockvisor_api::auth::rbac::AuthPerm;
use blockvisor_api::auth::token::refresh::Refresh;
use blockvisor_api::auth::token::RequestToken;
use blockvisor_api::database::seed::LOGIN_PASSWORD;
use blockvisor_api::grpc::api::{self, auth_service_client};
use blockvisor_api::models::user::User;

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type Service = auth_service_client::AuthServiceClient<Channel>;

#[tokio::test]
async fn ok_with_valid_credentials_for_login() {
    let test = TestServer::new().await;
    let req = api::AuthServiceLoginRequest {
        email: test.seed().user.email.clone(),
        password: LOGIN_PASSWORD.to_string(),
    };
    test.send(Service::login, req).await.unwrap();
}

#[tokio::test]
async fn fails_with_valid_credentials_for_unconfirmed_user_login() {
    let test = TestServer::new().await;
    let user = test.unconfirmed_user().await;
    let req = api::AuthServiceLoginRequest {
        email: user.email,
        password: LOGIN_PASSWORD.to_string(),
    };

    let status = test.send(Service::login, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);
}

#[tokio::test]
async fn unauthenticated_with_invalid_credentials_for_login() {
    let test = TestServer::new().await;
    let bogus = api::AuthServiceLoginRequest {
        email: "foo@bar.com".to_string(),
        password: "eafe12345".to_string(),
    };
    let status = test.send(Service::login, bogus).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn ok_with_valid_credentials_for_confirm() {
    let test = TestServer::new().await;
    let user = test.unconfirmed_user().await;

    let expires = chrono::Duration::minutes(15);
    let claims = Claims::from_now(expires, user.id, AuthPerm::Confirm);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let req = api::AuthServiceConfirmRequest {};
    test.send_with(Service::confirm, req, &jwt).await.unwrap();

    let mut conn = test.conn().await;
    let confirmed = User::is_confirmed(user.id, &mut conn).await.unwrap();
    assert!(confirmed);
}

#[tokio::test]
async fn ok_for_refresh() {
    let test = TestServer::new().await;

    let jwt = test.admin_jwt().await;
    let encoded = test.admin_encoded();
    let req = api::AuthServiceRefreshRequest {
        token: jwt.into(),
        refresh: Some(encoded.into()),
    };
    let resp = test.send_admin(Service::refresh, req).await.unwrap();

    let RequestToken::Bearer(token) = resp.token.parse().unwrap() else {
        panic!("Unexpected RequestToken type")
    };
    let refresh = resp.refresh.into();

    test.cipher().jwt.decode(&token).unwrap();
    test.cipher().refresh.decode(&refresh).unwrap();
}

#[tokio::test]
async fn ok_with_valid_password_for_update_ui_password() {
    let test = TestServer::new().await;
    let req = api::AuthServiceUpdateUiPasswordRequest {
        user_id: test.seed().user.id.to_string(),
        old_password: LOGIN_PASSWORD.to_string(),
        new_password: "hugo-boss".to_string(),
    };
    test.send_admin(Service::update_ui_password, req)
        .await
        .unwrap();
}

#[tokio::test]
async fn unauthenticated_with_invalid_old_password_for_update_ui_password() {
    let test = TestServer::new().await;
    let req = api::AuthServiceUpdateUiPasswordRequest {
        user_id: test.seed().user.id.to_string(),
        old_password: "some-wrong-pwd".to_string(),
        new_password: "hugo-boss".to_string(),
    };
    let status = test
        .send_admin(Service::update_ui_password, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn refresh_works() {
    let test = TestServer::new().await;

    let jwt = test.admin_jwt().await;
    let encoded = test.admin_encoded();
    let req = api::AuthServiceRefreshRequest {
        token: jwt.into(),
        refresh: Some(encoded.into()),
    };

    test.send(Service::refresh, req).await.unwrap();
}

#[tokio::test]
async fn refresh_works_from_cookie() {
    let test = TestServer::new().await;

    let jwt = test.admin_jwt().await;
    let expires = chrono::Duration::seconds(60);
    let refresh = Refresh::from_now(expires, test.seed().user.id);
    let encoded = test.cipher().refresh.encode(&refresh).unwrap();

    let req = api::AuthServiceRefreshRequest {
        token: jwt.into(),
        refresh: None,
    };
    let mut req = tonic::Request::new(req);
    req.metadata_mut()
        .insert("cookie", format!("refresh={}", *encoded).parse().unwrap());

    test.send(Service::refresh, req).await.unwrap();
}
