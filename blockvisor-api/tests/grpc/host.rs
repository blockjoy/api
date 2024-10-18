use blockvisor_api::database::seed::NODE_ID;
use blockvisor_api::grpc::api;
use blockvisor_api::model::host::{Host, UpdateHost};
use blockvisor_api::model::schema;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use tonic::Code;

use crate::setup::helper::traits::{HostService, NodeService, OrgService, SocketRpc};
use crate::setup::TestServer;

#[tokio::test]
async fn unauthenticated_without_token_for_update() {
    let test = TestServer::new().await;
    let req = api::HostServiceUpdateRequest {
        id: test.seed().host.id.to_string(),
        name: None,
        version: None,
        os: None,
        os_version: None,
        region: None,
        billing_amount: None,
        total_disk_space: None,
        managed_by: None,
        update_tags: None,
    };
    let status = test.send(HostService::update, req).await.unwrap_err();
    assert_eq!(status.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn permission_denied_with_token_ownership_for_update() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let other_host = test.host2().await;
    let req = api::HostServiceUpdateRequest {
        id: other_host.id.to_string(),
        name: Some("hostus mostus maximus".to_string()),
        version: Some("3".to_string()),
        os: Some("LuukOS".to_string()),
        os_version: Some("5".to_string()),
        region: None,
        billing_amount: None,
        total_disk_space: None,
        managed_by: None,
        update_tags: None,
    };

    let status = test
        .send_with(HostService::update, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn permission_denied_with_user_token_for_update() {
    let test = TestServer::new().await;

    let other_host = test.host2().await;
    let req = api::HostServiceUpdateRequest {
        id: other_host.id.to_string(),
        name: Some("hostus mostus maximus".to_string()),
        version: Some("3".to_string()),
        os: Some("LuukOS".to_string()),
        os_version: Some("5".to_string()),
        region: None,
        billing_amount: None,
        total_disk_space: None,
        managed_by: None,
        update_tags: None,
    };

    let status = test.send_admin(HostService::update, req).await.unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn ok_for_create() {
    let test = TestServer::new().await;
    let org_id = test.seed().org.id;
    let user_id = test.seed().user.id;

    let req = api::OrgServiceGetProvisionTokenRequest {
        org_id: org_id.to_string(),
        user_id: user_id.to_string(),
    };
    let provision_token = test
        .send_admin(OrgService::get_provision_token, req)
        .await
        .unwrap()
        .token;
    let req = api::HostServiceCreateRequest {
        provision_token,
        name: "test".to_string(),
        version: "3".to_string(),
        cpu_count: 2,
        mem_size_bytes: 2,
        disk_size_bytes: 2,
        os: "LuukOS".to_string(),
        os_version: "4".to_string(),
        ip_addr: "172.168.0.1".to_string(),
        ip_gateway: "72.168.0.100".to_string(),
        ips: vec!["172.168.0.2".to_string()],
        org_id: Some(org_id.to_string()),
        region: Some("europe-2-birmingham".to_string()),
        billing_amount: None,
        vmm_mountpoint: Some("/a/path/to/the/data/treasure".to_string()),
        managed_by: Some(api::ManagedBy::Automatic.into()),
        tags: None,
    };
    test.send(HostService::create, req).await.unwrap();
}

#[tokio::test]
async fn ok_for_update() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let req = api::HostServiceUpdateRequest {
        id: test.seed().host.id.to_string(),
        name: Some("Servy McServington".to_string()),
        version: Some("3".to_string()),
        os: Some("LuukOS".to_string()),
        os_version: Some("5".to_string()),
        region: None,
        billing_amount: None,
        total_disk_space: None,
        managed_by: None,
        update_tags: None,
    };

    test.send_with(HostService::update, req, &jwt)
        .await
        .unwrap();
}

#[tokio::test]
async fn ok_for_delete() {
    let test = TestServer::new().await;

    let req = api::HostServiceDeleteRequest {
        id: test.seed().host.id.to_string(),
    };

    // There is still a node. It shouldn't be possible to delete this host yet.
    let status = test
        .send_admin(HostService::delete, req.clone())
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::FailedPrecondition);

    let node_req = api::NodeServiceDeleteRequest {
        id: NODE_ID.to_string(),
    };
    test.send_admin(NodeService::delete, node_req)
        .await
        .unwrap();

    test.send_admin(HostService::delete, req).await.unwrap();
}

#[tokio::test]
async fn ok_for_start_stop_restart() {
    let test = TestServer::new().await;

    let jwt = test.admin_jwt().await;
    let host_id = test.seed().host.id;
    let req = api::HostServiceStartRequest {
        id: host_id.to_string(),
    };
    test.send_with(HostService::start, req, &jwt).await.unwrap();

    let req = api::HostServiceStopRequest {
        id: host_id.to_string(),
    };
    test.send_with(HostService::stop, req, &jwt).await.unwrap();

    let req = api::HostServiceRestartRequest {
        id: host_id.to_string(),
    };
    test.send_with(HostService::restart, req, &jwt)
        .await
        .unwrap();
}

#[tokio::test]
async fn unauthenticated_without_token_for_delete() {
    let test = TestServer::new().await;
    let req = api::HostServiceDeleteRequest {
        id: test.seed().host.id.to_string(),
    };
    let status = test.send(HostService::delete, req).await.unwrap_err();
    assert_eq!(status.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn permission_denied_for_delete() {
    let test = TestServer::new().await;

    let req = api::HostServiceDeleteRequest {
        id: test.seed().host.id.to_string(),
    };

    // now we generate a token for the wrong host.
    let other_host = test.host2().await;
    let claims = test.host_claims_for(other_host.id);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let status = test
        .send_with(HostService::delete, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn can_update_host_info() {
    use schema::hosts;

    let test = TestServer::new().await;
    let host = &test.seed().host;
    let update_host = UpdateHost {
        id: host.id,
        name: Some("test"),
        ip_gateway: Some("192.168.0.1".parse().unwrap()),
        version: None,
        cpu_count: None,
        mem_size_bytes: None,
        disk_size_bytes: None,
        os: None,
        os_version: None,
        ip_addr: None,
        status: None,
        region_id: None,
        managed_by: None,
        tags: None,
    };
    let mut conn = test.conn().await;
    let update = update_host.update(&mut conn).await.unwrap();
    assert_eq!(update.name, "test".to_string());

    // Fetch host after update to see if it really worked as expected

    let updated_host: Host = Host::not_deleted()
        .filter(hosts::id.eq(host.id))
        .get_result(&mut conn)
        .await
        .unwrap();

    assert_eq!(updated_host.name, "test".to_string());
    assert!(!updated_host.ip_addr.is_empty())
}

#[tokio::test]
async fn org_admin_can_view_billing_cost() {
    let test = TestServer::new().await;

    let id = test.seed().host.id.to_string();
    let req = api::HostServiceGetRequest { id };
    let resp = test.send_admin(HostService::get, req).await.unwrap();

    let billing_amount = resp.host.unwrap().billing_amount.unwrap();
    assert_eq!(billing_amount.amount.unwrap().value, 123)
}

#[tokio::test]
async fn org_member_cannot_view_billing_cost() {
    let test = TestServer::new().await;

    let id = test.seed().host.id.to_string();
    let req = api::HostServiceGetRequest { id };
    let resp = test.send_member(HostService::get, req).await.unwrap();

    assert!(resp.host.unwrap().billing_amount.is_none())
}
