use blockvisor_api::grpc::api;
use blockvisor_api::models;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

type Service = api::host_service_client::HostServiceClient<super::Channel>;

#[tokio::test]
async fn responds_unauthenticated_without_token_for_update() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;

    let req = api::HostServiceUpdateRequest {
        id: host.id.to_string(),
        name: None,
        version: None,
        os: None,
        os_version: None,
        region: None,
    };
    let status = tester.send(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_permission_denied_with_token_ownership_for_update() {
    let tester = super::Tester::new().await;

    let host = tester.host().await;
    let claims = tester.host_token(&host);
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let other_host = tester.host2().await;
    let req = api::HostServiceUpdateRequest {
        id: other_host.id.to_string(),
        name: Some("hostus mostus maximus".to_string()),
        version: Some("3".to_string()),
        os: Some("LuukOS".to_string()),
        os_version: Some("5".to_string()),
        region: None,
    };

    let status = tester
        .send_with(Service::update, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_permission_denied_with_user_token_for_update() {
    let tester = super::Tester::new().await;

    let user = tester.user().await;
    let claims = tester.user_token(&user).await;
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let other_host = tester.host2().await;
    let req = api::HostServiceUpdateRequest {
        id: other_host.id.to_string(),
        name: Some("hostus mostus maximus".to_string()),
        version: Some("3".to_string()),
        os: Some("LuukOS".to_string()),
        os_version: Some("5".to_string()),
        region: None,
    };

    let status = tester
        .send_with(Service::update, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_ok_for_create() {
    type OrgService = api::org_service_client::OrgServiceClient<super::Channel>;

    let tester = super::Tester::new().await;
    let org_id = tester.org().await.id.to_string();
    let req = api::OrgServiceGetProvisionTokenRequest {
        org_id: org_id.clone(),
        user_id: tester.user().await.id.to_string(),
    };
    let pwd = tester
        .send_admin(OrgService::get_provision_token, req)
        .await
        .unwrap()
        .token;
    let req = api::HostServiceCreateRequest {
        provision_token: pwd,
        name: "tester".to_string(),
        version: "3".to_string(),
        cpu_count: 2,
        mem_size_bytes: 2,
        disk_size_bytes: 2,
        os: "LuukOS".to_string(),
        os_version: "4".to_string(),
        ip_addr: "172.168.0.1".to_string(),
        ip_range_from: "172.168.0.1".to_string(),
        ip_range_to: "172.168.0.10".to_string(),
        ip_gateway: "72.168.0.100".to_string(),
        org_id: Some(org_id),
        region: Some("europe-2-birmingham".to_string()),
    };
    tester.send(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_update() {
    let tester = super::Tester::new().await;

    let host = tester.host().await;
    let claims = tester.host_token(&host);
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let req = api::HostServiceUpdateRequest {
        id: host.id.to_string(),
        name: Some("Servy McServington".to_string()),
        version: Some("3".to_string()),
        os: Some("LuukOS".to_string()),
        os_version: Some("5".to_string()),
        region: None,
    };

    tester.send_with(Service::update, req, &jwt).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_delete() {
    let tester = super::Tester::new().await;

    let host = tester.host().await;
    let claims = tester.host_token(&host);
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let req = api::HostServiceDeleteRequest {
        id: host.id.to_string(),
    };

    // There is still a node. It shouldn't be possible to delete this host yet.
    tester
        .send_with(Service::delete, req.clone(), &jwt)
        .await
        .unwrap_err();

    type NodeService = api::node_service_client::NodeServiceClient<super::Channel>;
    let node_req = api::NodeServiceDeleteRequest {
        id: "cdbbc736-f399-42ab-86cf-617ce983011d".to_string(),
    };
    tester
        .send_admin(NodeService::delete, node_req)
        .await
        .unwrap();
    tester.send_with(Service::delete, req, &jwt).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_start_stop_restart() {
    let tester = super::Tester::new().await;

    let host = tester.host().await;
    let user = tester.user().await;
    let claims = tester.user_token(&user).await;
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let req = api::HostServiceStartRequest {
        id: host.id.to_string(),
    };
    tester.send_with(Service::start, req, &jwt).await.unwrap();

    let req = api::HostServiceStopRequest {
        id: host.id.to_string(),
    };
    tester.send_with(Service::stop, req, &jwt).await.unwrap();

    let req = api::HostServiceRestartRequest {
        id: host.id.to_string(),
    };
    tester.send_with(Service::restart, req, &jwt).await.unwrap();
}

#[tokio::test]
async fn responds_unauthenticated_without_token_for_delete() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let req = api::HostServiceDeleteRequest {
        id: host.id.to_string(),
    };
    let status = tester.send(Service::delete, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_permission_denied_for_delete() {
    let tester = super::Tester::new().await;

    let host = tester.host().await;
    let req = api::HostServiceDeleteRequest {
        id: host.id.to_string(),
    };

    let other_host = tester.host2().await;
    // now we generate a token for the wrong host.
    let claims = tester.host_token(&other_host);
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let status = tester
        .send_with(Service::delete, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn can_update_host_info() {
    use models::schema::hosts;
    // TODO @Thomas: This doesn't really test the api, should this be here or maybe in
    // `src/models/host.rs`?

    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let update_host = models::UpdateHost {
        id: host.id,
        name: Some("tester"),
        ip_range_from: Some("192.168.0.10".parse().unwrap()),
        ip_range_to: Some("192.168.0.20".parse().unwrap()),
        ip_gateway: Some("192.168.0.1".parse().unwrap()),
        version: None,
        cpu_count: None,
        mem_size_bytes: None,
        disk_size_bytes: None,
        os: None,
        os_version: None,
        ip_addr: None,
        status: None,
    };
    let mut conn = tester.conn().await;
    let update = update_host.update(&mut conn).await.unwrap();
    assert_eq!(update.name, "tester".to_string());

    // Fetch host after update to see if it really worked as expected

    let updated_host: models::Host = hosts::table
        .filter(hosts::id.eq(host.id))
        .get_result(&mut conn)
        .await
        .unwrap();

    assert_eq!(updated_host.name, "tester".to_string());
    assert!(!updated_host.ip_addr.is_empty())
}
