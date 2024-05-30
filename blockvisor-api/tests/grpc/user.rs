use blockvisor_api::grpc::{api, common};
use blockvisor_api::models::User;
use tonic::transport::Channel;
use uuid::Uuid;

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type Service = api::user_service_client::UserServiceClient<Channel>;

#[tokio::test]
async fn responds_ok_with_valid_token_for_get() {
    let test = TestServer::new().await;
    let id = test.seed().user.id.to_string();
    let req = api::UserServiceGetRequest { id };
    test.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_with_valid_token_for_delete() {
    let test = TestServer::new().await;
    let user = &test.seed().user;
    let req = api::UserServiceDeleteRequest {
        id: user.id.to_string(),
    };
    test.send_admin(Service::delete, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_without_token_for_create() {
    let test = TestServer::new().await;
    let req = api::UserServiceCreateRequest {
        email: "hugo@boss.com".to_string(),
        first_name: "Hugo".to_string(),
        last_name: "The Bossman".to_string(),
        password: "abcde12345".to_string(),
    };
    test.send_admin(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_with_existing_email_for_create() {
    let test = TestServer::new().await;
    let user = &test.seed().user;
    let req = api::UserServiceCreateRequest {
        email: user.email.clone(),
        first_name: user.first_name.clone(),
        last_name: user.last_name.clone(),
        password: "abcde12345".to_string(),
    };
    let status = test.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::AlreadyExists);
}

#[tokio::test]
async fn responds_permission_denied_with_diff_users_for_update() {
    let test = TestServer::new().await;
    let req = api::UserServiceUpdateRequest {
        id: test.unconfirmed_user().await.id.to_string(),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
    };
    let status = test.send_admin(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_ok_with_equal_users_for_update() {
    let test = TestServer::new().await;
    let user = &test.seed().user;
    let req = api::UserServiceUpdateRequest {
        id: user.id.to_string(),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
    };
    test.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn can_confirm_unconfirmed_user() {
    let test = TestServer::new().await;
    let user = test.unconfirmed_user().await;

    assert!(user.confirmed_at.is_none());

    let mut conn = test.conn().await;
    User::confirm(user.id, &mut conn).await.unwrap();
    let user = User::by_id(user.id, &mut conn).await.unwrap();

    user.confirmed_at.unwrap();
}

#[tokio::test]
async fn cannot_confirm_confirmed_user() {
    let test = TestServer::new().await;

    let user = &test.seed().user;
    user.confirmed_at.unwrap();

    let mut conn = test.conn().await;
    User::confirm(user.id, &mut conn)
        .await
        .expect_err("Already confirmed user confirmed again");
}

#[tokio::test]
async fn can_check_if_user_confirmed() {
    let test = TestServer::new().await;
    let user = test.unconfirmed_user().await;

    assert!(user.confirmed_at.is_none());

    let mut conn = test.conn().await;
    User::confirm(user.id, &mut conn).await.unwrap();
    let user = User::by_id(user.id, &mut conn).await.unwrap();

    assert!(user.confirmed_at.is_some());
    assert!(User::is_confirmed(user.id, &mut conn).await.unwrap());
}

#[tokio::test]
async fn returns_false_for_unconfirmed_user_at_check_if_user_confirmed() {
    let test = TestServer::new().await;
    let user = test.unconfirmed_user().await;

    assert!(user.confirmed_at.is_none());
    let mut conn = test.conn().await;
    assert!(!User::is_confirmed(user.id, &mut conn).await.unwrap());
}

#[tokio::test]
async fn test_billing() {
    const TOTALLY_REAL_BILLING_ID: &str = "the most billy of ids";

    let test = TestServer::new().await;
    let user = &test.seed().user;
    assert!(user.chargebee_billing_id.is_none());

    // Test that we indeed get no billing id back
    let get = api::UserServiceGetBillingRequest {
        user_id: user.id.to_string(),
    };
    let resp = test
        .send_admin(Service::get_billing, get.clone())
        .await
        .unwrap();
    assert!(resp.billing_id.is_none());

    // Test that we can set a billing id
    let update = api::UserServiceUpdateBillingRequest {
        user_id: user.id.to_string(),
        billing_id: Some(TOTALLY_REAL_BILLING_ID.to_string()),
    };
    let resp = test
        .send_admin(Service::update_billing, update)
        .await
        .unwrap();
    assert!(resp.billing_id.is_some());

    // Test that we can retrieve said billing id
    let resp = test
        .send_admin(Service::get_billing, get.clone())
        .await
        .unwrap();
    assert!(resp.billing_id.is_some());

    // Test that we can delete the billing id
    let delete = api::UserServiceDeleteBillingRequest {
        user_id: user.id.to_string(),
    };
    test.send_admin(Service::delete_billing, delete)
        .await
        .unwrap();

    // Test that it indeed is gone
    let resp = test.send_admin(Service::get_billing, get).await.unwrap();
    assert!(resp.billing_id.is_none());
}

#[tokio::test]
async fn test_list() {
    let test = TestServer::new().await;

    let org_id = test.seed().org.id.to_string();
    let fake_org_id = Uuid::new_v4().to_string();
    let req = |org_id, email_like| api::UserServiceListRequest {
        org_id,
        offset: 0,
        limit: 10,
        search: Some(api::UserSearch {
            operator: common::v1::SearchOperator::And.into(),
            email: email_like,
            ..Default::default()
        }),
        sort: vec![],
    };

    // Test that an org member can list our own org.
    let resp = test
        .send_member(Service::list, req(Some(org_id), None))
        .await
        .unwrap();
    assert_eq!(resp.users.len(), 3, "{resp:?}");

    // Test that an org member cannot list other organizations
    test.send_member(Service::list, req(Some(fake_org_id.clone()), None))
        .await
        .unwrap_err();

    // Test that a blockjoy admin can list for other organizations.
    let resp = test
        .send_root(Service::list, req(Some(fake_org_id), None))
        .await
        .unwrap();
    assert_eq!(resp.users.len(), 0, "{resp:?}");

    // Test that root can list by email
    let resp = test
        .send_root(Service::list, req(None, Some("admin%".to_string())))
        .await
        .unwrap();
    assert_eq!(resp.users.len(), 1, "{resp:?}");

    // Test that we don't get matches when there are none
    let resp = test
        .send_root(Service::list, req(None, Some("admin".to_string())))
        .await
        .unwrap();
    assert_eq!(resp.users.len(), 0, "{resp:?}");
}
