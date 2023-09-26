use blockvisor_api::auth::claims::{Claims, Expirable};
use blockvisor_api::auth::rbac::InvitationPerm;
use blockvisor_api::auth::resource::ResourceEntry;
use blockvisor_api::database::seed;
use blockvisor_api::grpc::api;
use blockvisor_api::models::invitation::{Invitation, NewInvitation};
use blockvisor_api::models::org::Org;
use tonic::transport::Channel;

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type Service = api::invitation_service_client::InvitationServiceClient<Channel>;

async fn create_invitation(test: &TestServer) -> Invitation {
    let mut conn = test.conn().await;
    let new_invitation = NewInvitation {
        created_by: test.seed().user.id,
        org_id: test.seed().org.id,
        invitee_email: seed::UNCONFIRMED_EMAIL.to_string(),
    };
    new_invitation.create(&mut conn).await.unwrap()
}

#[tokio::test]
async fn can_create_new_invitation() {
    let test = TestServer::new().await;
    let req = api::InvitationServiceCreateRequest {
        invitee_email: "hugo@boss.com".to_string(),
        org_id: test.seed().org.id.to_string(),
    };
    test.send_admin(Service::create, req).await.unwrap();

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
        status: Some(api::InvitationStatus::Open.into()),
        ..Default::default()
    };

    test.send_admin(Service::list, req).await.unwrap();

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
        invitee_email: Some(test.seed().user.email.to_string()),
        ..Default::default()
    };

    test.send_admin(Service::list, req).await.unwrap();
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

    let resource = ResourceEntry::new_org(invitation.org_id).into();
    let expirable = Expirable::from_now(chrono::Duration::minutes(15));
    let access = InvitationPerm::Accept.into();

    let data = hashmap! { "email".into() => invitation.invitee_email };
    let claims = Claims::new(resource, expirable, access).with_data(data);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let req = api::InvitationServiceAcceptRequest {
        invitation_id: invitation.id.to_string(),
    };

    test.send_with(Service::accept, req, &jwt).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_decline() {
    let test = TestServer::new().await;

    let invitation = create_invitation(&test).await;

    let resource = ResourceEntry::new_org(invitation.org_id).into();
    let expirable = Expirable::from_now(chrono::Duration::minutes(15));
    let access = InvitationPerm::Decline.into();

    let data = hashmap! { "email".into() => invitation.invitee_email };
    let claims = Claims::new(resource, expirable, access).with_data(data);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let req = api::InvitationServiceDeclineRequest {
        invitation_id: invitation.id.to_string(),
    };

    test.send_with(Service::decline, req, &jwt).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_revoke() {
    let test = TestServer::new().await;
    let invitation = create_invitation(&test).await;
    let mut conn = test.conn().await;
    let org = Org::find_by_id(invitation.org_id, &mut conn).await.unwrap();
    // If the user is already added, thats okay
    let _ = Org::add_member(&org, test.seed().user.id, &mut conn).await;
    let req = api::InvitationServiceRevokeRequest {
        invitation_id: invitation.id.to_string(),
    };

    test.send_admin(Service::revoke, req).await.unwrap();
}
