use blockvisor_api::{grpc::api, models};

type Service = api::user_service_client::UserServiceClient<super::Channel>;

#[tokio::test]
async fn responds_ok_with_valid_token_for_get() {
    let tester = super::Tester::new().await;
    let id = tester.user().await.id.to_string();
    let req = api::UserServiceGetRequest { id };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_with_valid_token_for_delete() {
    let tester = super::Tester::new().await;
    let admin = tester.user().await;
    let req = api::UserServiceDeleteRequest {
        id: admin.id.to_string(),
    };
    tester.send_admin(Service::delete, req).await.unwrap();
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
    let user = tester.user().await;
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
        id: tester.unconfirmed_user().await.id.to_string(),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
    };
    let status = tester.send_admin(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_ok_with_equal_users_for_update() {
    let tester = super::Tester::new().await;
    let user = tester.user().await;
    let req = api::UserServiceUpdateRequest {
        id: user.id.to_string(),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn can_confirm_unconfirmed_user() {
    let tester = super::Tester::new().await;
    let user = tester.unconfirmed_user().await;

    assert!(user.confirmed_at.is_none());

    let mut conn = tester.conn().await;
    models::User::confirm(user.id, &mut conn).await.unwrap();
    let user = models::User::find_by_id(user.id, &mut conn).await.unwrap();

    user.confirmed_at.unwrap();
}

#[tokio::test]
async fn cannot_confirm_confirmed_user() {
    let tester = super::Tester::new().await;
    let user = tester.user().await;
    let mut conn = tester.conn().await;
    user.confirmed_at.unwrap();
    models::User::confirm(user.id, &mut conn)
        .await
        .expect_err("Already confirmed user confirmed again");
}

#[tokio::test]
async fn can_check_if_user_confirmed() {
    let tester = super::Tester::new().await;
    let user = tester.unconfirmed_user().await;

    assert!(user.confirmed_at.is_none());

    let mut conn = tester.conn().await;
    models::User::confirm(user.id, &mut conn).await.unwrap();
    let user = models::User::find_by_id(user.id, &mut conn).await.unwrap();

    assert!(user.confirmed_at.is_some());
    assert!(models::User::is_confirmed(user.id, &mut conn)
        .await
        .unwrap());
}

#[tokio::test]
async fn returns_false_for_unconfirmed_user_at_check_if_user_confirmed() {
    let tester = super::Tester::new().await;
    let user = tester.unconfirmed_user().await;

    assert!(user.confirmed_at.is_none());
    let mut conn = tester.conn().await;
    assert!(!models::User::is_confirmed(user.id, &mut conn)
        .await
        .unwrap());
}
