use blockvisor_api::auth::{self, FindableById};
use blockvisor_api::grpc::api;
use blockvisor_api::models;

type Service = api::invitation_service_client::InvitationServiceClient<super::Channel>;

async fn create_invitation(tester: &super::Tester) -> anyhow::Result<models::Invitation> {
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let new_invitation = models::NewInvitation {
        created_by_user: user.id,
        created_by_user_name: user.last_name,
        created_for_org: org.id,
        created_for_org_name: org.name,
        invitee_email: "test@here.com",
    };
    let mut conn = tester.conn().await;
    let inv = new_invitation.create(&mut conn).await.unwrap();
    Ok(inv)
}

#[tokio::test]
async fn responds_ok_for_create() {
    let tester = super::Tester::new().await;
    let org_id = tester.org().await.id;
    let req = api::InvitationServiceCreateRequest {
        invitee_email: "hugo@boss.com".to_string(),
        org_id: org_id.to_string(),
    };

    tester.send_admin(Service::create, req).await.unwrap();

    let mut conn = tester.conn().await;
    let cnt = models::Invitation::received("hugo@boss.com", &mut conn)
        .await
        .unwrap()
        .len();

    assert_eq!(cnt, 1);
}

#[tokio::test]
async fn responds_ok_for_list_pending() {
    let tester = super::Tester::new().await;
    let invitation = create_invitation(&tester).await.unwrap();
    let user = tester.admin_user().await;
    let org = tester.org_for(&user).await;
    let req = api::InvitationServiceListRequest {
        org_id: Some(org.id.to_string()),
        status: Some(api::InvitationStatus::Open.into()),
        ..Default::default()
    };

    tester.send_admin(Service::list, req).await.unwrap();

    let mut conn = tester.conn().await;
    let invitations = models::Invitation::received(&invitation.invitee_email, &mut conn)
        .await
        .unwrap();

    assert_eq!(invitations.len(), 1);
}

#[tokio::test]
async fn responds_ok_for_list_received() {
    let tester = super::Tester::new().await;
    let invitation = create_invitation(&tester).await.unwrap();
    let user = tester.admin_user().await;
    let req = api::InvitationServiceListRequest {
        invitee_id: Some(user.id.to_string()),
        ..Default::default()
    };

    tester.send_admin(Service::list, req).await.unwrap();
    let mut conn = tester.conn().await;
    let invitations = models::Invitation::received(&invitation.invitee_email, &mut conn)
        .await
        .unwrap();

    assert_eq!(invitations.len(), 1);
}

#[tokio::test]
async fn responds_ok_for_accept() {
    let tester = super::Tester::new().await;
    let invitation = create_invitation(&tester).await.unwrap();
    let token = auth::InvitationToken::create_for_invitation(&invitation).unwrap();
    let req = api::InvitationServiceAcceptRequest {
        invitation_id: invitation.id.to_string(),
    };

    tester
        .send_with(Service::accept, req, token, super::DummyRefresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_ok_for_decline() {
    let tester = super::Tester::new().await;
    let invitation = create_invitation(&tester).await.unwrap();
    let token = auth::InvitationToken::create_for_invitation(&invitation).unwrap();

    let req = api::InvitationServiceDeclineRequest {
        invitation_id: invitation.id.to_string(),
    };

    tester
        .send_with(Service::decline, req, token, super::DummyRefresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_ok_for_revoke() {
    let tester = super::Tester::new().await;
    let invitation = create_invitation(&tester).await.unwrap();
    let user = tester.admin_user().await;
    let mut conn = tester.conn().await;
    let org = models::Org::find_by_id(invitation.created_for_org, &mut conn)
        .await
        .unwrap();
    // If the user is already added, thats okay
    let _ = models::Org::add_member(user.id, org.id, models::OrgRole::Admin, &mut conn).await;
    let req = api::InvitationServiceRevokeRequest {
        invitation_id: invitation.id.to_string(),
    };

    tester.send_admin(Service::revoke, req).await.unwrap();
}
