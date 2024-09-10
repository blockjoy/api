use blockvisor_api::auth::resource::HostId;
use blockvisor_api::database::seed::NODE_ID;
use blockvisor_api::grpc::api;
use tonic::Code;

use crate::setup::helper::traits::{HostService, NodeService, OrgService, SocketRpc};
use crate::setup::TestServer;

#[tokio::test]
async fn create_a_new_host() {
    let test = TestServer::new().await;

    let create_req = |provision_token| api::HostServiceCreateRequest {
        provision_token,
        org_id: None,
        network_name: "new-host".to_string(),
        display_name: None,
        region: Some("europe-2-birmingham".to_string()),
        schedule_type: api::ScheduleType::Automatic as i32,
        os: "LuukOS".to_string(),
        os_version: "4".to_string(),
        bv_version: "0.1.2".to_string(),
        ip_address: "172.168.0.1".to_string(),
        ip_gateway: "72.168.0.100".to_string(),
        ips: vec!["172.168.0.2".to_string()],
        cpu_cores: 2,
        memory_bytes: 2,
        disk_bytes: 2,
        tags: None,
    };

    // fails with invalid provision token
    let req = create_req("invalid".into());
    let status = test
        .send_unauthenticated(HostService::create, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);

    let provision_req = api::OrgServiceGetProvisionTokenRequest {
        org_id: test.seed().org.id.to_string(),
        user_id: test.seed().member.id.to_string(),
    };
    let provision_token = test
        .send_admin(OrgService::get_provision_token, provision_req)
        .await
        .unwrap()
        .token;

    // ok with valid provision token
    let req = create_req(provision_token);
    let resp = test
        .send_unauthenticated(HostService::create, req)
        .await
        .unwrap();
    assert_eq!(resp.host.unwrap().network_name, "new-host");
}

#[tokio::test]
async fn update_an_existing_host() {
    let test = TestServer::new().await;

    let update_req = |host_id: HostId| api::HostServiceUpdateRequest {
        host_id: host_id.to_string(),
        org_id: None,
        network_name: None,
        display_name: Some("Servy McServington".to_string()),
        region: None,
        schedule_type: None,
        os: Some("TempleOS".to_string()),
        os_version: Some("3".to_string()),
        bv_version: Some("0.1.2".to_string()),
        cpu_cores: None,
        memory_bytes: None,
        disk_bytes: None,
        update_tags: None,
    };

    // fails without token
    let req = update_req(test.seed().host1.id);
    let status = test
        .send_unauthenticated(HostService::update, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::Unauthenticated);

    // denied with org-admin token
    let req = update_req(test.seed().host2.id);
    let status = test.send_admin(HostService::update, req).await.unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);

    // denied with wrong host token
    let jwt = test.host_jwt();
    let req = update_req(test.seed().host2.id);
    let status = test
        .send_with(HostService::update, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);

    // ok for correct host
    let req = update_req(test.seed().host1.id);
    test.send_with(HostService::update, req, &jwt)
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_an_existing_host() {
    let test = TestServer::new().await;

    let delete_req = |host_id: HostId| api::HostServiceDeleteRequest {
        host_id: host_id.to_string(),
        org_id: None,
    };

    // fails without token
    let req = delete_req(test.seed().host1.id);
    let status = test
        .send_unauthenticated(HostService::delete, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::Unauthenticated);

    // fails for the wrong host
    let jwt = test.host_jwt();
    let req = delete_req(test.seed().host2.id);
    let status = test
        .send_with(HostService::delete, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);

    // fails while there is still a node
    let req = delete_req(test.seed().host1.id);
    let status = test
        .send_admin(HostService::delete, req.clone())
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::FailedPrecondition);

    let node_req = api::NodeServiceDeleteRequest {
        node_id: NODE_ID.to_string(),
    };
    test.send_admin(NodeService::delete, node_req)
        .await
        .unwrap();

    // ok once nodes are deleted
    test.send_admin(HostService::delete, req).await.unwrap();
}

#[tokio::test]
async fn start_and_stop_a_host() {
    let test = TestServer::new().await;

    let req = api::HostServiceStartRequest {
        host_id: test.seed().host1.id.to_string(),
        org_id: None,
    };
    test.send_admin(HostService::start, req).await.unwrap();

    let req = api::HostServiceStopRequest {
        host_id: test.seed().host1.id.to_string(),
        org_id: None,
    };
    test.send_admin(HostService::stop, req).await.unwrap();

    let req = api::HostServiceRestartRequest {
        host_id: test.seed().host1.id.to_string(),
        org_id: None,
    };
    test.send_admin(HostService::restart, req).await.unwrap();
}
