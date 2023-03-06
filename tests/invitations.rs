mod setup;

use api::auth::FindableById;
use api::grpc::blockjoy_ui::{self, invitation_service_client};
use api::models;
use tonic::transport;

type Service = invitation_service_client::InvitationServiceClient<transport::Channel>;

#[tokio::test]
async fn cannot_create_invitation_without_valid_props() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::CreateInvitationRequest {
        created_for_org_id: "".to_string(),
        invitee_email: "please@me.com".to_string(),
        meta: None,
    };

    tester.send_admin(Service::create, req).await.unwrap_err();
}

#[tokio::test]
async fn can_create_invitation_with_valid_props() {
    let tester = setup::Tester::new().await;
    let org = tester.org().await;
    let req = blockjoy_ui::CreateInvitationRequest {
        created_for_org_id: org.id.to_string(),
        invitee_email: "please@me.com".to_string(),
        meta: None,
    };

    tester.send_admin(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn can_list_pending_invitations() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let new_invite = models::NewInvitation {
        created_by_user: user.id,
        created_for_org: org.id,
        created_by_user_name: "hugo".to_string(),
        created_for_org_name: "boss".to_string(),
        invitee_email: "hugo@boss.com",
    };
    let mut conn = tester.conn().await;
    new_invite.create(&mut conn).await.unwrap();

    let req = blockjoy_ui::ListPendingInvitationRequest {
        org_id: org.id.to_string(),
        meta: None,
    };
    let invitations = tester.send_admin(Service::list_pending, req).await.unwrap();

    assert_eq!(invitations.invitations.len(), 1);
}

#[tokio::test]
async fn can_list_received_invitations() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let new_invite = models::NewInvitation {
        created_by_user: user.id,
        created_for_org: org.id,
        created_by_user_name: "hugo".to_string(),
        created_for_org_name: "boss".to_string(),
        invitee_email: &user.email,
    };
    let mut conn = tester.conn().await;
    new_invite.create(&mut conn).await.unwrap();

    let req = blockjoy_ui::ListReceivedInvitationRequest {
        user_id: user.id.to_string(),
        meta: None,
    };
    let invitations = tester
        .send_admin(Service::list_received, req)
        .await
        .unwrap();

    assert_eq!(invitations.invitations.len(), 1);
}

#[tokio::test]
async fn can_accept_invitation() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let new_invite = models::NewInvitation {
        created_by_user: user.id,
        created_for_org: org.id,
        created_by_user_name: "hugo".to_string(),
        created_for_org_name: "boss".to_string(),
        invitee_email: "test@here.com",
    };
    let mut conn = tester.conn().await;
    let invite = new_invite.create(&mut conn).await.unwrap();

    let req = blockjoy_ui::InvitationRequest {
        invitation: Some(blockjoy_ui::Invitation {
            id: Some(invite.id.to_string()),
            ..Default::default()
        }),
        meta: None,
    };
    tester.send_admin(Service::accept, req).await.unwrap();

    let invite = models::Invitation::find_by_id(invite.id, &mut conn)
        .await
        .unwrap();
    invite.accepted_at.unwrap();
}

#[tokio::test]
async fn can_decline_invitation() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let new_invite = models::NewInvitation {
        created_by_user: user.id,
        created_for_org: org.id,
        created_by_user_name: "hugo".to_string(),
        created_for_org_name: "boss".to_string(),
        invitee_email: &user.email,
    };
    let mut conn = tester.conn().await;
    let invite = new_invite.create(&mut conn).await.unwrap();

    let req = blockjoy_ui::InvitationRequest {
        invitation: Some(blockjoy_ui::Invitation {
            id: Some(invite.id.to_string()),
            ..Default::default()
        }),
        meta: None,
    };
    tester.send_admin(Service::decline, req).await.unwrap();

    let invite = models::Invitation::find_by_id(invite.id, &mut conn)
        .await
        .unwrap();
    invite.declined_at.unwrap();
}

#[tokio::test]
async fn can_revoke_invitation() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let new_invite = models::NewInvitation {
        created_by_user: user.id,
        created_for_org: org.id,
        created_by_user_name: "hugo".to_string(),
        created_for_org_name: "boss".to_string(),
        invitee_email: &user.email,
    };
    let mut conn = tester.conn().await;
    let invite = new_invite.create(&mut conn).await.unwrap();

    let req = blockjoy_ui::InvitationRequest {
        invitation: Some(blockjoy_ui::Invitation {
            invitee_email: Some(user.email),
            ..Default::default()
        }),
        meta: None,
    };
    tester.send_admin(Service::revoke, req).await.unwrap();

    models::Invitation::find_by_id(invite.id, &mut conn)
        .await
        .unwrap_err();
}
