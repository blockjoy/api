mod setup;

use api::grpc::blockjoy_ui::Invitation as GrpcInvitation;
use api::models::Invitation;

#[tokio::test]
async fn cannot_create_invitation_without_valid_props() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let grpc_invitation = GrpcInvitation {
        id: None,
        created_by_id: None,
        created_for_org_id: None,
        invitee_email: None,
        created_at: None,
        accepted_at: None,
        declined_at: None,
        created_by_user_name: None,
        created_for_org_name: None,
    };

    let mut tx = tester.begin().await;
    Invitation::create(&grpc_invitation, &mut tx)
        .await
        .expect_err("This shouldn't work");
    tx.commit().await?;
    Ok(())
}

#[tokio::test]
async fn can_create_invitation_with_valid_props() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let grpc_invitation = GrpcInvitation {
        id: None,
        created_by_id: Some(user.id.to_string()),
        created_for_org_id: Some(org.id.to_string()),
        invitee_email: Some("hugo@boss.com".to_string()),
        created_at: None,
        accepted_at: None,
        declined_at: None,
        created_by_user_name: Some("hugo".to_string()),
        created_for_org_name: Some("boss".to_string()),
    };
    let mut tx = tester.begin().await;
    Invitation::create(&grpc_invitation, &mut tx).await?;
    tx.commit().await.unwrap();

    Ok(())
}

#[tokio::test]
async fn can_list_pending_invitations() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let grpc_invitation = GrpcInvitation {
        id: None,
        created_by_id: Some(user.id.to_string()),
        created_for_org_id: Some(org.id.to_string()),
        invitee_email: Some("hugo@boss.com".to_string()),
        created_at: None,
        accepted_at: None,
        declined_at: None,
        created_by_user_name: Some("hugo".to_string()),
        created_for_org_name: Some("boss".to_string()),
    };
    let mut tx = tester.begin().await;
    Invitation::create(&grpc_invitation, &mut tx).await?;
    let invitations = Invitation::pending(org.id, &mut tx).await?;
    tx.commit().await.unwrap();

    assert_eq!(invitations.len(), 1);

    Ok(())
}

#[tokio::test]
async fn can_list_received_invitations() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let grpc_invitation = GrpcInvitation {
        id: None,
        created_by_id: Some(user.id.to_string()),
        created_for_org_id: Some(org.id.to_string()),
        invitee_email: Some("hugo@boss.com".to_string()),
        created_at: None,
        accepted_at: None,
        declined_at: None,
        created_by_user_name: Some("hugo".to_string()),
        created_for_org_name: Some("boss".to_string()),
    };
    let mut tx = tester.begin().await;
    Invitation::create(&grpc_invitation, &mut tx).await?;
    let invitations = Invitation::received("hugo@boss.com", &mut tx).await?;
    tx.commit().await.unwrap();

    assert_eq!(invitations.len(), 1);

    Ok(())
}

#[tokio::test]
async fn can_accept_invitation() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let grpc_invitation = GrpcInvitation {
        id: None,
        created_by_id: Some(user.id.to_string()),
        created_for_org_id: Some(org.id.to_string()),
        invitee_email: Some("hugo@boss.com".to_string()),
        created_at: None,
        accepted_at: None,
        declined_at: None,
        created_by_user_name: Some("hugo".to_string()),
        created_for_org_name: Some("boss".to_string()),
    };
    let mut tx = tester.begin().await;
    let invitation = Invitation::create(&grpc_invitation, &mut tx).await?;
    let invitation = Invitation::accept(invitation.id, &mut tx).await?;
    tx.commit().await.unwrap();

    invitation.accepted_at.unwrap();

    Ok(())
}

#[tokio::test]
async fn can_decline_invitation() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let grpc_invitation = GrpcInvitation {
        id: None,
        created_by_id: Some(user.id.to_string()),
        created_for_org_id: Some(org.id.to_string()),
        invitee_email: Some("hugo@boss.com".to_string()),
        created_at: None,
        accepted_at: None,
        declined_at: None,
        created_by_user_name: Some("hugo".to_string()),
        created_for_org_name: Some("boss".to_string()),
    };
    let mut tx = tester.begin().await;
    let invitation = Invitation::create(&grpc_invitation, &mut tx).await?;
    let invitation = Invitation::decline(invitation.id, &mut tx).await?;
    tx.commit().await.unwrap();

    invitation.declined_at.unwrap();

    Ok(())
}

#[tokio::test]
async fn can_revoke_invitation() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org = tester.org().await;
    let grpc_invitation = GrpcInvitation {
        id: None,
        created_by_id: Some(user.id.to_string()),
        created_for_org_id: Some(org.id.to_string()),
        invitee_email: Some("hugo@boss.com".to_string()),
        created_at: None,
        accepted_at: None,
        declined_at: None,
        created_by_user_name: Some("hugo".to_string()),
        created_for_org_name: Some("boss".to_string()),
    };
    let mut tx = tester.begin().await;
    let invitation = Invitation::create(&grpc_invitation, &mut tx).await?;
    let invitation_id = invitation.id;

    Invitation::revoke(invitation_id, &mut tx).await?;

    let cnt: i32 = sqlx::query_scalar("select count(*)::int from invitations where id = $1")
        .bind(invitation_id)
        .fetch_one(&mut tx)
        .await?;
    tx.commit().await.unwrap();

    assert_eq!(cnt, 0);

    Ok(())
}
