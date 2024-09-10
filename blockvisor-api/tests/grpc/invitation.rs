use blockvisor_api::auth::claims::{Claims, Expirable};
use blockvisor_api::auth::rbac::{InvitationPerm, OrgRole};
use blockvisor_api::auth::resource::Resource;
use blockvisor_api::database::seed;
use blockvisor_api::grpc::api;
use blockvisor_api::model::invitation::{Invitation, NewInvitation};
use blockvisor_api::model::org::Org;

use crate::setup::helper::traits::{InvitationService, SocketRpc};
use crate::setup::TestServer;

async fn create_invitation(test: &TestServer) -> Invitation {
    let mut conn = test.conn().await;

    let user_id = test.seed().admin.id;
    let org_id = test.seed().org.id;

    let new_invitation = NewInvitation::new(org_id, seed::UNCONFIRMED_EMAIL, user_id);
    new_invitation.create(&mut conn).await.unwrap()
}

#[tokio::test]
async fn can_create_new_invitation() {
    let test = TestServer::new().await;
    let req = api::InvitationServiceCreateRequest {
        invitee_email: "hugo@boss.com".to_string(),
        org_id: test.seed().org.id.to_string(),
    };
    test.send_admin(InvitationService::create, req)
        .await
        .unwrap();

    let mut conn = test.conn().await;
    let invitations = Invitation::received("hugo@boss.com", &mut conn)
        .await
        .unwrap();
    assert_eq!(invitations.len(), 1);
}

#[tokio::test]
async fn responds_ok_for_list_pending() {
    let test = TestServer::new().await;
    let invitation = create_invitation(&test).await;
    let req = api::InvitationServiceListRequest {
        org_id: Some(test.seed().org.id.to_string()),
        status: api::InvitationStatus::Open.into(),
        ..Default::default()
    };

    test.send_admin(InvitationService::list, req).await.unwrap();

    let mut conn = test.conn().await;
    let invitations = Invitation::received(&invitation.invitee_email, &mut conn)
        .await
        .unwrap();

    assert_eq!(invitations.len(), 1);
}

#[tokio::test]
async fn responds_ok_for_list_received() {
    let test = TestServer::new().await;
    let invitation = create_invitation(&test).await;
    let req = api::InvitationServiceListRequest {
        invitee_email: Some(test.seed().admin.email.to_string()),
        ..Default::default()
    };

    test.send_admin(InvitationService::list, req).await.unwrap();
    let mut conn = test.conn().await;
    let invitations = Invitation::received(&invitation.invitee_email, &mut conn)
        .await
        .unwrap();

    assert_eq!(invitations.len(), 1);
}

#[tokio::test]
async fn responds_ok_for_accept() {
    let test = TestServer::new().await;

    let invitation = create_invitation(&test).await;

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
}

#[tokio::test]
async fn responds_ok_for_decline() {
    let test = TestServer::new().await;

    let invitation = create_invitation(&test).await;

    let resource = Resource::Org(invitation.org_id);
    let expirable = Expirable::from_now(chrono::Duration::minutes(15));
    let access = InvitationPerm::Decline.into();

    let data = hashmap! { "email".into() => invitation.invitee_email };
    let claims = Claims::new(resource, expirable, access).with_data(data);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let req = api::InvitationServiceDeclineRequest {
        invitation_id: invitation.id.to_string(),
    };

    test.send_with(InvitationService::decline, req, &jwt)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_ok_for_revoke() {
    let test = TestServer::new().await;
    let invitation = create_invitation(&test).await;
    let mut conn = test.conn().await;
    let org = Org::by_id(invitation.org_id, &mut conn).await.unwrap();
    // If the user is already added, thats okay
    let _ = Org::add_user(test.seed().member.id, org.id, OrgRole::Member, &mut conn).await;
    let req = api::InvitationServiceRevokeRequest {
        invitation_id: invitation.id.to_string(),
    };

    test.send_admin(InvitationService::revoke, req)
        .await
        .unwrap();
}
