use blockvisor_api::auth::resource::UserId;
use blockvisor_api::grpc::{api, common};
use blockvisor_api::model::User;
use tonic::Code;
use uuid::Uuid;

use crate::setup::helper::traits::{SocketRpc, UserService};
use crate::setup::TestServer;

#[tokio::test]
async fn create_a_new_user() {
    let test = TestServer::new().await;

    let create_req = |email: &str| api::UserServiceCreateRequest {
        email: email.to_string(),
        first_name: "Hugo".to_string(),
        last_name: "Boss".to_string(),
        password: "abcde12345".to_string(),
    };

    // fails for existing email
    let req = create_req(&test.seed().member.email);
    let status = test
        .send_member(UserService::create, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::AlreadyExists);

    // ok for new email
    let req = create_req("hugo@boss.com");
    test.send_member(UserService::create, req).await.unwrap();
}

#[tokio::test]
async fn confirm_a_new_user() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    // cannot confirm a user that is already confirmed
    let user = &test.seed().member;
    assert!(user.confirmed_at.is_some());
    User::confirm(user.id, &mut conn)
        .await
        .expect_err("Already confirmed");

    // can confirm an unconfirmed user
    let user = test.unconfirmed_user().await;
    assert!(user.confirmed_at.is_none());
    assert!(!User::is_confirmed(user.id, &mut conn).await.unwrap());

    User::confirm(user.id, &mut conn).await.unwrap();
    let user = User::by_id(user.id, &mut conn).await.unwrap();
    assert!(user.confirmed_at.is_some());
    assert!(User::is_confirmed(user.id, &mut conn).await.unwrap());
}

#[tokio::test]
async fn get_an_exiting_user() {
    let test = TestServer::new().await;

    let get_req = |user_id| api::UserServiceGetRequest { user_id };

    // can't get an unknown user
    let req = get_req(Uuid::new_v4().to_string());
    let status = test.send_member(UserService::get, req).await.unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);

    // can get a known user
    let req = get_req(test.seed().member.id.to_string());
    test.send_member(UserService::get, req).await.unwrap();
}

#[tokio::test]
async fn delete_an_existing_user() {
    let test = TestServer::new().await;

    let delete_req = |user_id| api::UserServiceDeleteRequest { user_id };

    // can't delete an unknown user
    let req = delete_req(Uuid::new_v4().to_string());
    let status = test
        .send_member(UserService::delete, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);

    // can delete an existing user
    let req = delete_req(test.seed().member.id.to_string());
    test.send_member(UserService::delete, req).await.unwrap();
}

#[tokio::test]
async fn update_an_existing_user() {
    let test = TestServer::new().await;

    let update_req = |user_id: UserId| api::UserServiceUpdateRequest {
        user_id: user_id.to_string(),
        first_name: Some("Hugo".to_string()),
        last_name: Some("Boss".to_string()),
    };

    // cannot update unconfirmed user
    let req = update_req(test.unconfirmed_user().await.id);
    let status = test
        .send_member(UserService::update, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);

    // can update a confirmed user
    let req = update_req(test.seed().member.id);
    test.send_member(UserService::update, req).await.unwrap();
}

#[tokio::test]
async fn list_users() {
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
        .send_member(UserService::list, req(Some(org_id), None))
        .await
        .unwrap();
    assert_eq!(resp.users.len(), 3, "{resp:?}");

    // Test that an org member cannot list other organizations
    test.send_member(UserService::list, req(Some(fake_org_id.clone()), None))
        .await
        .unwrap_err();

    // Test that a super user can list for other organizations.
    let resp = test
        .send_super(UserService::list, req(Some(fake_org_id), None))
        .await
        .unwrap();
    assert_eq!(resp.users.len(), 0, "{resp:?}");

    // Test that root can list by email
    let resp = test
        .send_super(UserService::list, req(None, Some("admin%".to_string())))
        .await
        .unwrap();
    assert_eq!(resp.users.len(), 1, "{resp:?}");

    // Test that we don't get matches when there are none
    let resp = test
        .send_super(UserService::list, req(None, Some("admin".to_string())))
        .await
        .unwrap();
    assert_eq!(resp.users.len(), 0, "{resp:?}");
}
