mod setup;

use api::grpc::blockjoy_ui::{self, organization_service_client};
use tonic::transport;

type Service = organization_service_client::OrganizationServiceClient<transport::Channel>;

#[tokio::test]
async fn responds_ok_for_create() {
    let tester = setup::Tester::new().await;
    let org = blockjoy_ui::Organization {
        name: Some("new-org".to_string()),
        id: None,
        created_at: None,
        updated_at: None,
        member_count: None,
        personal: None,
        current_user: None,
    };
    let req = blockjoy_ui::CreateOrganizationRequest {
        meta: Some(tester.meta()),
        organization: Some(org),
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
    let org = blockjoy_ui::Organization {
        name: Some("new-org-asdf".to_string()),
        id: Some(org_id),
        created_at: None,
        updated_at: None,
        member_count: None,
        personal: None,
        current_user: None,
    };
    let req = blockjoy_ui::UpdateOrganizationRequest {
        meta: Some(tester.meta()),
        organization: Some(org),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_for_delete_on_personal_org() {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let org_id = tester.org_for(&user).await.id.to_string();
    let req = blockjoy_ui::DeleteOrganizationRequest {
        meta: Some(tester.meta()),
        id: org_id,
    };
    let result = tester.send_admin(Service::delete, req).await;

    match result {
        Err(status) => assert_eq!(status.code(), tonic::Code::NotFound),
        Ok(_) => panic!("This may not work"),
    }
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
