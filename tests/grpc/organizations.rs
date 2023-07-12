use std::collections::HashMap;

use blockvisor_api::auth::claims::{Claims, Expirable};
use blockvisor_api::auth::endpoint::{Endpoint, Endpoints};
use blockvisor_api::auth::resource::ResourceEntry;
use blockvisor_api::grpc::api;
use blockvisor_api::models::invitation::NewInvitation;
use blockvisor_api::models::org::Org;

type Service = api::org_service_client::OrgServiceClient<super::Channel>;

#[tokio::test]
async fn responds_ok_for_create() {
    let tester = super::Tester::new().await;
    let req = api::OrgServiceCreateRequest {
        name: "new-org".to_string(),
    };
    tester.send_admin(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_get() {
    let tester = super::Tester::new().await;
    let admin = tester.user().await;
    let id = tester.org_for(&admin).await.id.to_string();
    let req = api::OrgServiceGetRequest { id };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_update() {
    let tester = super::Tester::new().await;
    let user = tester.user().await;
    let org_id = tester.org_for(&user).await.id.to_string();
    let req = api::OrgServiceUpdateRequest {
        id: org_id,
        name: Some("new-org-asdf".to_string()),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn delete_org() {
    let tester = super::Tester::new().await;
    let req = api::OrgServiceCreateRequest {
        name: "new-org".to_string(),
    };
    let org = tester.send_admin(Service::create, req).await.unwrap();

    let req = api::OrgServiceDeleteRequest {
        id: org.org.unwrap().id,
    };
    tester.send_admin(Service::delete, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_for_delete_on_personal_org() {
    let tester = super::Tester::new().await;
    let user = tester.user().await;
    let mut conn = tester.conn().await;
    let org = Org::find_personal_org(&user, &mut conn).await.unwrap();
    let req = api::OrgServiceDeleteRequest {
        id: org.id.to_string(),
    };
    let status = tester.send_admin(Service::delete, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn member_count_works() {
    use blockvisor_api::grpc::api::invitation_service_client;
    type InvService = invitation_service_client::InvitationServiceClient<super::Channel>;

    let tester = super::Tester::new().await;

    let tester = &tester;
    let user = tester.user().await;
    let org = tester.org().await;

    // First we check that we do in fact start out with one member in the org.
    let req = api::OrgServiceGetRequest {
        id: org.id.to_string(),
    };
    let resp = tester.send_admin(Service::get, req).await.unwrap();
    assert_eq!(resp.org.unwrap().member_count, 1);

    // Now we invite someone new.
    let new_invitation = NewInvitation {
        created_by: user.id,
        org_id: org.id,
        invitee_email: "test@here.com".to_string(),
    };
    let mut conn = tester.conn().await;
    let invitation = new_invitation.create(&mut conn).await.unwrap();

    let resource = ResourceEntry::new_org(invitation.org_id).into();
    let expirable = Expirable::from_now(chrono::Duration::minutes(15));
    let endpoints = Endpoints::Multiple(vec![Endpoint::InvitationAccept]);

    let claims = Claims::new(resource, expirable, endpoints)
        .with_data(HashMap::from([("email".into(), invitation.invitee_email)]));
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let req = api::InvitationServiceAcceptRequest {
        invitation_id: invitation.id.to_string(),
    };

    tester
        .send_with(InvService::accept, req, &jwt)
        .await
        .unwrap();

    // Now we can check that there are actually two peeps in the org.
    let req = api::OrgServiceGetRequest {
        id: org.id.to_string(),
    };
    let resp = tester.send_admin(Service::get, req).await.unwrap();
    let n_members = resp.org.unwrap().member_count;
    assert_eq!(n_members, 2);

    // Now we perform the same assertion for querying in bulk:
    let req = api::OrgServiceListRequest {
        member_id: Some(user.id.to_string()),
    };
    let resp = tester.send_admin(Service::list, req).await.unwrap();
    let org_resp = resp
        .orgs
        .into_iter()
        .find(|o| o.id == org.id.to_string())
        .unwrap();
    let n_members = org_resp.member_count;
    assert_eq!(n_members, 2);
}
