use blockvisor_api::auth::claims::{Claims, Expirable};
use blockvisor_api::auth::rbac::InvitationPerm;
use blockvisor_api::auth::resource::Resource;
use blockvisor_api::database::seed;
use blockvisor_api::grpc::api;
use blockvisor_api::model::invitation::NewInvitation;
use blockvisor_api::model::org::Org;

use crate::setup::helper::traits::{InvitationService, OrgService, SocketRpc};
use crate::setup::TestServer;

#[tokio::test]
async fn can_create_new_org() {
    let test = TestServer::new().await;
    let req = api::OrgServiceCreateRequest {
        name: "new-org".to_string(),
    };
    test.send_admin(OrgService::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_get() {
    let test = TestServer::new().await;
    let org_id = test.seed().org.id.to_string();
    let req = api::OrgServiceGetRequest { org_id };
    test.send_admin(OrgService::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_update() {
    let test = TestServer::new().await;
    let req = api::OrgServiceUpdateRequest {
        org_id: test.seed().org.id.to_string(),
        name: Some("new-org-asdf".to_string()),
    };
    test.send_admin(OrgService::update, req).await.unwrap();
}

#[tokio::test]
async fn delete_org() {
    let test = TestServer::new().await;
    let req = api::OrgServiceCreateRequest {
        name: "new-org".to_string(),
    };
    let org = test.send_admin(OrgService::create, req).await.unwrap();

    let req = api::OrgServiceDeleteRequest {
        org_id: org.org.unwrap().org_id,
    };
    test.send_admin(OrgService::delete, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_for_delete_on_personal_org() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    let user_id = test.seed().member.id;
    let org = Org::find_personal(user_id, &mut conn).await.unwrap();

    let req = api::OrgServiceDeleteRequest {
        org_id: org.id.to_string(),
    };
    let status = test.send_admin(OrgService::delete, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn member_count_works() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    let user_id = test.seed().member.id;
    let org_id = test.seed().org.id;

    // First we get the current org member count.
    let req = api::OrgServiceGetRequest {
        org_id: org_id.to_string(),
    };
    let resp = test.send_member(OrgService::get, req).await.unwrap();
    let members = resp.org.unwrap().member_count;

    // Now we invite someone new.
    let new_invitation = NewInvitation::new(org_id, seed::UNCONFIRMED_EMAIL, user_id);
    let invitation = new_invitation.create(&mut conn).await.unwrap();

    let resource = Resource::Org(invitation.org_id);
    let expirable = Expirable::from_now(chrono::Duration::minutes(15));
    let access = InvitationPerm::Accept.into();
    let data = hashmap! { "email".into() => invitation.invitee_email };

    let claims = Claims::new(resource, expirable, access).with_data(data);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let req = api::InvitationServiceAcceptRequest {
        invitation_id: invitation.id.to_string(),
    };
    test.send_with(InvitationService::accept, req, &jwt)
        .await
        .unwrap();

    // Now we can check that there is one more org member
    let req = api::OrgServiceGetRequest {
        org_id: org_id.to_string(),
    };
    let resp = test.send_member(OrgService::get, req).await.unwrap();
    assert_eq!(resp.org.unwrap().member_count, members + 1);

    // Now we perform the same assertion for querying in bulk:
    let req = api::OrgServiceListRequest {
        member_id: Some(user_id.to_string()),
        personal: None,
        offset: 0,
        limit: 10,
        search: None,
        sort: vec![],
    };
    let resp = test.send_member(OrgService::list, req).await.unwrap();
    let org_resp = resp
        .orgs
        .into_iter()
        .find(|o| o.org_id == org_id.to_string())
        .unwrap();
    assert_eq!(org_resp.member_count, members + 1);
}
