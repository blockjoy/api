mod setup;

use api::{
    auth,
    grpc::blockjoy_ui::{self, organization_service_client},
    models,
};
use tonic::transport;

type Service = organization_service_client::OrganizationServiceClient<transport::Channel>;

#[tokio::test]
async fn responds_ok_for_create() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::CreateOrganizationRequest {
        meta: Some(tester.meta()),
        name: "new-org".to_string(),
    };
    tester.send_admin(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_get() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::GetOrganizationsRequest {
        org_id: None,
        meta: Some(tester.meta()),
    };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_single_get() {
    let tester = setup::Tester::new().await;
    let admin = tester.admin_user().await;
    let org_id = tester.org_for(&admin).await.id.to_string();
    let req = blockjoy_ui::GetOrganizationsRequest {
        org_id: Some(org_id),
        meta: Some(tester.meta()),
    };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_update() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org_id = tester.org_for(&user).await.id.to_string();
    let req = blockjoy_ui::UpdateOrganizationRequest {
        meta: Some(tester.meta()),
        id: org_id,
        name: Some("new-org-asdf".to_string()),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_for_delete_on_personal_org() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let mut conn = tester.conn().await;
    let org = models::Org::find_personal_org(user.id, &mut conn)
        .await
        .unwrap();
    let req = blockjoy_ui::DeleteOrganizationRequest {
        meta: Some(tester.meta()),
        id: org.id.to_string(),
    };
    let status = tester.send_admin(Service::delete, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_ok_for_members() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org_id = tester.org_for(&user).await.id.to_string();
    let req = blockjoy_ui::OrganizationMemberRequest {
        meta: Some(tester.meta()),
        id: org_id,
    };
    tester.send_admin(Service::members, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_with_pagination_for_members() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org_id = tester.org_for(&user).await.id.to_string();
    let req = blockjoy_ui::OrganizationMemberRequest {
        meta: Some(tester.meta().with_pagination(tester.pagination())),
        id: org_id,
    };

    let resp = tester.send_admin(Service::members, req).await.unwrap();

    let max_items: i32 = std::env::var("PAGINATION_MAX_ITEMS")
        .expect("MAX ITEMS NOT SET")
        .parse()
        .expect("Could not parse max_items :(");

    let meta = resp.meta.unwrap();
    let pagination = meta.pagination.expect("`meta` field is empty");

    assert_eq!(pagination.items_per_page, max_items);
    assert_eq!(pagination.current_page, 0);
    assert_eq!(pagination.total_items.unwrap(), 0);
}

#[tokio::test]
async fn member_count_works() {
    use api::grpc::blockjoy_ui::invitation_service_client;
    type InvService = invitation_service_client::InvitationServiceClient<transport::Channel>;

    let tester = setup::Tester::new().await;

    let tester = &tester;
    let user = tester.admin_user().await;
    let org = tester.org().await;

    // First we check that we do in fact start out with one member in the org.
    let req = blockjoy_ui::GetOrganizationsRequest {
        org_id: Some(org.id.to_string()),
        meta: Some(tester.meta()),
    };
    let mut resp = tester.send_admin(Service::get, req).await.unwrap();
    let n_members = resp.organizations.pop().unwrap().member_count;
    assert_eq!(n_members, 1);

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
    let grpc_invitation = blockjoy_ui::Invitation {
        id: Some(invitation.id.to_string()),
        ..Default::default()
    };
    let req = blockjoy_ui::InvitationRequest {
        meta: Some(tester.meta()),
        invitation: Some(grpc_invitation),
    };

    tester
        .send_with(InvService::accept, req, token, setup::DummyRefresh)
        .await
        .unwrap();

    // Now we can check that there are actually two peeps in the org.
    let req = blockjoy_ui::GetOrganizationsRequest {
        org_id: Some(org.id.to_string()),
        meta: Some(tester.meta()),
    };
    let mut resp = tester.send_admin(Service::get, req).await.unwrap();
    let n_members = resp.organizations.pop().unwrap().member_count;
    assert_eq!(n_members, 2);

    // Now we perform the same assertion for querying in bulk:
    let req = blockjoy_ui::GetOrganizationsRequest {
        org_id: None,
        meta: Some(tester.meta()),
    };
    let resp = tester.send_admin(Service::get, req).await.unwrap();
    let org_resp = resp
        .organizations
        .into_iter()
        .find(|o| o.id == org.id.to_string())
        .unwrap();
    let n_members = org_resp.member_count;
    assert_eq!(n_members, 2);
}
