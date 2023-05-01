use blockvisor_api::{
    auth::{self, JwtToken},
    grpc::api,
    models,
};

type Service = api::user_service_client::UserServiceClient<super::Channel>;

#[tokio::test]
async fn responds_ok_with_valid_token_for_get() {
    let tester = super::Tester::new().await;
    let id = tester.admin_user().await.id.to_string();
    let req = api::UserServiceGetRequest { id };
    tester.send_admin(Service::get, req).await.unwrap();
}

/// THOMAS: why should this test fail? It should be allowed for this user to delete themselves
/// right?
#[tokio::test]
#[ignore]
async fn responds_not_found_with_valid_token_for_delete() {
    let tester = super::Tester::new().await;
    let req = api::UserServiceDeleteRequest {};
    let status = tester.send_admin(Service::delete, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_ok_with_valid_token_for_delete() {
    let tester = super::Tester::new().await;
    let req = api::UserServiceDeleteRequest {};

    tester.send_admin(Service::delete, req).await.unwrap();
}

#[tokio::test]
async fn responds_unauthenticated_without_valid_token_for_get() {
    let tester = super::Tester::new().await;
    let token = base64::encode("some-invalid-token");
    let token = super::DummyToken(&token);
    let id = tester.admin_user().await.id.to_string();
    let req = api::UserServiceGetRequest { id };
    let status = tester
        .send_with(Service::get, req, token, super::DummyRefresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_ok_without_token_for_create() {
    let tester = super::Tester::new().await;
    let req = api::UserServiceCreateRequest {
        email: "hugo@boss.com".to_string(),
        first_name: "Hugo".to_string(),
        last_name: "The Bossman".to_string(),
        password: "abcde12345".to_string(),
    };
    tester.send(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_with_existing_email_for_create() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;
    let req = api::UserServiceCreateRequest {
        email: user.email,
        first_name: user.first_name,
        last_name: user.last_name,
        password: "abcde12345".to_string(),
    };
    let status = tester.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_permission_denied_with_diff_users_for_update() {
    let tester = super::Tester::new().await;
    let req = api::UserServiceUpdateRequest {
        id: uuid::Uuid::new_v4().to_string(),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
    };
    let status = tester.send_admin(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_ok_with_equal_users_for_update() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;
    let req = api::UserServiceUpdateRequest {
        id: user.id.to_string(),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn can_verify_and_refresh_auth_token() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;
    let claim = auth::TokenClaim::new(
        user.id,
        chrono::Utc::now().timestamp() + 60000,
        auth::TokenType::UserRefresh,
        auth::TokenRole::User,
        None,
    );
    let refresh_token = auth::UserRefreshToken::try_new(claim).unwrap();
    let encoded = refresh_token.encode().unwrap();
    let fields = models::UpdateUser {
        id: user.id,
        first_name: None,
        last_name: None,
        staking_quota: None,
        refresh: Some(&encoded),
    };
    let mut conn = tester.conn().await;
    let user = fields.update(&mut conn).await.unwrap();
    let claim = auth::TokenClaim::new(
        user.id,
        chrono::Utc::now().timestamp() - 1,
        auth::TokenType::UserAuth,
        auth::TokenRole::User,
        None,
    );
    let auth = auth::UserAuthToken::try_new(claim).unwrap();

    models::User::verify_and_refresh_auth_token(auth, refresh_token, &mut conn)
        .await
        .unwrap();
}

#[tokio::test]
async fn cannot_verify_and_refresh_wo_valid_refresh_token() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;
    let claim = auth::TokenClaim::new(
        user.id,
        chrono::Utc::now().timestamp() - 60000,
        auth::TokenType::UserRefresh,
        auth::TokenRole::User,
        None,
    );
    let refresh_token = auth::UserRefreshToken::try_new(claim).unwrap();
    let encoded = refresh_token.encode().unwrap();
    let fields = models::UpdateUser {
        id: user.id,
        first_name: None,
        last_name: None,
        staking_quota: None,
        refresh: Some(&encoded),
    };
    let mut conn = tester.conn().await;
    let user = fields.update(&mut conn).await.unwrap();
    let claim = auth::TokenClaim::new(
        user.id,
        chrono::Utc::now().timestamp() - 1,
        auth::TokenType::UserAuth,
        auth::TokenRole::User,
        None,
    );
    let auth_token = auth::UserAuthToken::try_new(claim).unwrap();

    models::User::verify_and_refresh_auth_token(auth_token, refresh_token, &mut conn)
        .await
        .unwrap_err();
}

#[tokio::test]
async fn can_confirm_unconfirmed_user() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;

    assert!(user.confirmed_at.is_none());

    let mut conn = tester.conn().await;
    let user = models::User::confirm(user.id, &mut conn).await.unwrap();

    user.confirmed_at.unwrap();
}

#[tokio::test]
async fn cannot_confirm_confirmed_user() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;

    assert!(user.confirmed_at.is_none());

    let mut conn = tester.conn().await;
    let user = models::User::confirm(user.id, &mut conn).await.unwrap();

    assert!(user.confirmed_at.is_some());

    models::User::confirm(user.id, &mut conn)
        .await
        .expect_err("Already confirmed user confirmed again");
}

#[tokio::test]
async fn can_check_if_user_confirmed() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;

    assert!(user.confirmed_at.is_none());

    let mut conn = tester.conn().await;
    let user = models::User::confirm(user.id, &mut conn).await.unwrap();

    assert!(user.confirmed_at.is_some());
    assert!(models::User::is_confirmed(user.id, &mut conn)
        .await
        .unwrap());
}

#[tokio::test]
async fn returns_false_for_unconfirmed_user_at_check_if_user_confirmed() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;

    assert!(user.confirmed_at.is_none());
    let mut conn = tester.conn().await;
    assert!(!models::User::is_confirmed(user.id, &mut conn)
        .await
        .unwrap());
}
