use std::collections::HashSet;

use blockvisor_api::auth::rbac::OrgRole;
use blockvisor_api::auth::resource::{OrgId, Resource};
use blockvisor_api::grpc::api;
use blockvisor_api::model::rbac::RbacUser;

use crate::setup::helper::rpc;
use crate::setup::helper::traits::{OrgService, SocketRpc};
use crate::setup::TestServer;

#[tokio::test]
async fn grpc_login_role_can_create_api_key() {
    let mut test = TestServer::new().await;

    let org_user = rpc::new_org_user(&mut test).await;
    let resp = rpc::create_api_key(
        &test,
        &org_user.jwt,
        Resource::User(org_user.user_id),
        "label",
    )
    .await;
    assert!(resp.is_ok())
}

#[tokio::test]
async fn org_owner_can_delete_org() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;
    let org_id = test.seed().org.id;

    // org admin cannot delete org
    let req = api::OrgServiceDeleteRequest {
        org_id: org_id.to_string(),
    };
    let status = test.send_admin(OrgService::delete, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);

    // org owner can delete org
    RbacUser::link_role(test.seed().admin.id, org_id, OrgRole::Owner, &mut conn)
        .await
        .unwrap();

    let req = api::OrgServiceDeleteRequest {
        org_id: org_id.to_string(),
    };
    let resp = test.send_admin(OrgService::delete, req).await;
    assert!(resp.is_ok());
}

#[tokio::test]
async fn blockjoy_admin_can_list_all_orgs() {
    let test = TestServer::new().await;

    let org_id = test.seed().org.id;
    let org_ids = |resp: api::OrgServiceListResponse| {
        resp.orgs
            .into_iter()
            .map(|org| org.org_id.parse().unwrap())
            .collect::<HashSet<OrgId>>()
    };
    let make_req = |id| api::OrgServiceListRequest {
        member_id: id,
        personal: None,
        offset: 0,
        limit: 10,
        search: None,
        sort: vec![],
    };

    // org admin can list own org
    let req = make_req(Some(test.seed().admin.id.to_string()));
    let resp = test.send_admin(OrgService::list, req).await.unwrap();
    assert!(org_ids(resp).contains(&org_id));

    // org admin cannot list all orgs
    let req = make_req(None);
    let status = test.send_admin(OrgService::list, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);

    // super user can list all orgs
    let req = make_req(None);
    let resp = test.send_super(OrgService::list, req).await.unwrap();
    assert!(org_ids(resp).contains(&org_id));
}
