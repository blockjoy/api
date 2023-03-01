mod setup;

use crate::setup::Tester;
use api::auth::{FindableById, InvitationToken};
use api::grpc::blockjoy_ui::{self, invitation_service_client, Invitation as GrpcInvitation};
use api::models;
use api::models::{Org, OrgRole};
use tonic::transport;

type Service = invitation_service_client::InvitationServiceClient<transport::Channel>;

async fn create_invitation(tester: &Tester) -> anyhow::Result<models::Invitation> {
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let new_invitation = models::NewInvitation {
        created_by_user: user.id,
        created_by_user_name: user.last_name,
        created_for_org: org.id,
        created_for_org_name: org.org.name,
        invitee_email: "test@here.com",
    };
    let mut conn = tester.conn().await;
    let inv = new_invitation.create(&mut conn).await?;
    Ok(inv)
}

#[tokio::test]
async fn responds_ok_for_create() -> anyhow::Result<()> {
    let tester = Tester::new().await;
    let org_id = tester.org().await.id;
    let req = blockjoy_ui::CreateInvitationRequest {
        meta: Some(tester.meta()),
        invitee_email: "hugo@boss.com".to_string(),
        created_for_org_id: org_id.to_string(),
    };

    tester.send_admin(Service::create, req).await?;

    let mut conn = tester.conn().await;
    let cnt = models::Invitation::received("hugo@boss.com", &mut conn)
        .await?
        .len();

    assert_eq!(cnt, 1);

    Ok(())
}

#[tokio::test]
async fn responds_ok_for_list_pending() -> anyhow::Result<()> {
    let tester = Tester::new().await;
    let invitation = create_invitation(&tester).await?;
    let user = tester.admin_user().await;
    let org = tester.org_for(&user).await;
    let req = blockjoy_ui::ListPendingInvitationRequest {
        meta: Some(tester.meta()),
        org_id: org.id.to_string(),
    };

    tester.send_admin(Service::list_pending, req).await?;

    let mut conn = tester.conn().await;
    let invitations = models::Invitation::received(&invitation.invitee_email, &mut conn).await?;

    assert_eq!(invitations.len(), 1);

    Ok(())
}

#[tokio::test]
async fn responds_ok_for_list_received() -> anyhow::Result<()> {
    let tester = Tester::new().await;
    let invitation = create_invitation(&tester).await?;
    let req = blockjoy_ui::ListReceivedInvitationRequest {
        meta: Some(tester.meta()),
        // TODO: Remove user_id from protos
        user_id: "".to_string(),
    };

    tester.send_admin(Service::list_received, req).await?;
    let mut conn = tester.conn().await;
    let invitations = models::Invitation::received(&invitation.invitee_email, &mut conn).await?;

    assert_eq!(invitations.len(), 1);

    Ok(())
}

#[tokio::test]
async fn responds_ok_for_accept() -> anyhow::Result<()> {
    let tester = Tester::new().await;
    let invitation = create_invitation(&tester).await?;
    let token = InvitationToken::create_for_invitation(&invitation)?;
    let grpc_invitation = GrpcInvitation {
        id: Some(invitation.id.to_string()),
        ..Default::default()
    };
    let req = blockjoy_ui::InvitationRequest {
        meta: Some(tester.meta()),
        invitation: Some(grpc_invitation),
    };

    tester
        .send_with(Service::accept, req, token, setup::DummyRefresh)
        .await?;

    Ok(())
}

#[tokio::test]
async fn responds_ok_for_decline() -> anyhow::Result<()> {
    let tester = Tester::new().await;
    let invitation = create_invitation(&tester).await?;
    let token = InvitationToken::create_for_invitation(&invitation)?;

    let grpc_invitation = GrpcInvitation {
        id: Some(invitation.id.to_string()),
        created_by_id: Some(invitation.created_by_user.to_string()),
        created_for_org_id: Some(invitation.created_for_org.to_string()),
        invitee_email: Some(invitation.invitee_email.to_string()),
        created_at: None,
        accepted_at: None,
        declined_at: None,
        created_by_user_name: Some("hugo".to_string()),
        created_for_org_name: Some("boss".to_string()),
    };
    let req = blockjoy_ui::InvitationRequest {
        meta: Some(tester.meta()),
        invitation: Some(grpc_invitation),
    };

    tester
        .send_with(Service::decline, req, token, setup::DummyRefresh)
        .await?;

    Ok(())
}

#[tokio::test]
async fn responds_ok_for_revoke() -> anyhow::Result<()> {
    let tester = Tester::new().await;
    let invitation = create_invitation(&tester).await?;
    let user = tester.admin_user().await;
    let mut conn = tester.conn().await;
    let org = Org::find_by_id(invitation.created_for_org, &mut conn).await?;
    // If the user is already added, thats okay
    let _ = Org::add_member(user.id, org.id, OrgRole::Admin, &mut conn).await;
    let grpc_invitation = GrpcInvitation {
        id: Some(invitation.id.to_string()),
        created_by_id: Some(invitation.created_by_user.to_string()),
        created_for_org_id: Some(invitation.created_for_org.to_string()),
        invitee_email: Some(invitation.invitee_email.to_string()),
        created_at: None,
        accepted_at: None,
        declined_at: None,
        created_by_user_name: Some("hugo".to_string()),
        created_for_org_name: Some("boss".to_string()),
    };
    let req = blockjoy_ui::InvitationRequest {
        meta: Some(tester.meta()),
        invitation: Some(grpc_invitation),
    };

    tester.send_admin(Service::revoke, req).await?;

    Ok(())
}
