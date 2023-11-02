use tonic::transport::Channel;

use blockvisor_api::auth::claims::{Claims, Expirable};
use blockvisor_api::auth::rbac::InvitationPerm;
use blockvisor_api::auth::resource::ResourceEntry;
use blockvisor_api::database::seed;
use blockvisor_api::grpc::api::{self, invitation_service_client};
use blockvisor_api::models::invitation::NewInvitation;
use blockvisor_api::models::org::Org;

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type Service = api::org_service_client::OrgServiceClient<Channel>;
type InvitationService = invitation_service_client::InvitationServiceClient<Channel>;

#[tokio::test]
async fn can_create_new_org() {
    let test = TestServer::new().await;
    let req = api::OrgServiceCreateRequest {
        name: "new-org".to_string(),
    };
    test.send_admin(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_get() {
    let test = TestServer::new().await;
    let id = test.seed().org.id.to_string();
    let req = api::OrgServiceGetRequest { id };
    test.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_update() {
    let test = TestServer::new().await;
    let req = api::OrgServiceUpdateRequest {
        id: test.seed().org.id.to_string(),
        name: Some("new-org-asdf".to_string()),
    };
    test.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn delete_org() {
    let test = TestServer::new().await;
    let req = api::OrgServiceCreateRequest {
        name: "new-org".to_string(),
    };
    let org = test.send_admin(Service::create, req).await.unwrap();

    let req = api::OrgServiceDeleteRequest {
        id: org.org.unwrap().id,
    };
    test.send_admin(Service::delete, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_for_delete_on_personal_org() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    let user_id = test.seed().user.id;
    let org = Org::find_personal(user_id, &mut conn).await.unwrap();

    let req = api::OrgServiceDeleteRequest {
        id: org.id.to_string(),
    };
    let status = test.send_admin(Service::delete, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn member_count_works() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    let user_id = test.seed().user.id;
    let org_id = test.seed().org.id;

    // First we get the current org member count.
    let req = api::OrgServiceGetRequest {
        id: org_id.to_string(),
    };
    let resp = test.send_admin(Service::get, req).await.unwrap();
    let members = resp.org.unwrap().member_count;

    // Now we invite someone new.
    let new_invitation = NewInvitation::new(org_id, seed::UNCONFIRMED_EMAIL, user_id);
    let invitation = new_invitation.create(&mut conn).await.unwrap();

    let resource = ResourceEntry::new_org(invitation.org_id).into();
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
        id: org_id.to_string(),
    };
    let resp = test.send_admin(Service::get, req).await.unwrap();
    assert_eq!(resp.org.unwrap().member_count, members + 1);

    // Now we perform the same assertion for querying in bulk:
    let req = api::OrgServiceListRequest {
        member_id: Some(user_id.to_string()),
        personal: None,
        offset: 0,
        limit: 10,
        search: None,
    };
    let resp = test.send_admin(Service::list, req).await.unwrap();
    let org_resp = resp
        .orgs
        .into_iter()
        .find(|o| o.id == org_id.to_string())
        .unwrap();
    assert_eq!(org_resp.member_count, members + 1);
}
