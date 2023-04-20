use blockvisor_api::{auth, grpc::api, models};

type Service = api::orgs_client::OrgsClient<super::Channel>;

#[tokio::test]
async fn responds_ok_for_create() {
    let tester = super::Tester::new().await;
    let req = api::CreateOrgRequest {
        name: "new-org".to_string(),
    };
    tester.send_admin(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_get() {
    let tester = super::Tester::new().await;
    let admin = tester.admin_user().await;
    let org_id = tester.org_for(&admin).await.id.to_string();
    let req = api::GetOrgRequest { org_id };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_update() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;
    let org_id = tester.org_for(&user).await.id.to_string();
    let req = api::UpdateOrgRequest {
        id: org_id,
        name: Some("new-org-asdf".to_string()),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_for_delete_on_personal_org() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;
    let mut conn = tester.conn().await;
    let org = models::Org::find_personal_org(user.id, &mut conn)
        .await
        .unwrap();
    let req = api::DeleteOrgRequest {
        id: org.id.to_string(),
    };
    let status = tester.send_admin(Service::delete, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_ok_for_members() {
    let tester = super::Tester::new().await;
    let user = tester.admin_user().await;
    let org_id = tester.org_for(&user).await.id.to_string();
    let req = api::OrgMembersRequest { id: org_id };
    tester.send_admin(Service::members, req).await.unwrap();
}

#[tokio::test]
async fn member_count_works() {
    use blockvisor_api::grpc::api::invitations_client;
    type InvService = invitations_client::InvitationsClient<super::Channel>;

    let tester = super::Tester::new().await;

    let tester = &tester;
    let user = tester.admin_user().await;
    let org = tester.org().await;

    // First we check that we do in fact start out with one member in the org.
    let req = api::GetOrgRequest {
        org_id: org.id.to_string(),
    };
    let resp = tester.send_admin(Service::get, req).await.unwrap();
    assert_eq!(resp.org.unwrap().member_count, 1);

    // Now we invite someone new.
    let new_invitation = models::NewInvitation {
        created_by_user: user.id,
        created_by_user_name: user.last_name,
        created_for_org: org.id,
        created_for_org_name: org.name.clone(),
        invitee_email: "test@here.com",
    };
    let mut conn = tester.conn().await;
    let invitation = new_invitation.create(&mut conn).await.unwrap();

    let token = auth::InvitationToken::create_for_invitation(&invitation).unwrap();
    let req = api::AcceptInvitationRequest {
        invitation_id: invitation.id.to_string(),
    };

    tester
        .send_with(InvService::accept, req, token, super::DummyRefresh)
        .await
        .unwrap();

    // Now we can check that there are actually two peeps in the org.
    let req = api::GetOrgRequest {
        org_id: org.id.to_string(),
    };
    let resp = tester.send_admin(Service::get, req).await.unwrap();
    let n_members = resp.org.unwrap().member_count;
    assert_eq!(n_members, 2);

    // Now we perform the same assertion for querying in bulk:
    let req = api::ListOrgsRequest { member_id: None };
    let resp = tester.send_admin(Service::list, req).await.unwrap();
    let org_resp = resp
        .orgs
        .into_iter()
        .find(|o| o.id == org.id.to_string())
        .unwrap();
    let n_members = org_resp.member_count;
    assert_eq!(n_members, 2);
}
