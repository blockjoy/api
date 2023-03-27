mod setup;

use api::grpc::blockjoy::{self, host_service_client};
use api::models;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use tonic::transport;

type Service = host_service_client::HostServiceClient<transport::Channel>;

#[tokio::test]
async fn responds_unauthenticated_with_empty_token_for_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let req = blockjoy::HostUpdateRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        id: host.id.to_string(),
        ip: Some("123.456.789.0".into()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
        os: None,
        os_version: None,
        version: None,
    };
    let status = tester
        .send_with(
            Service::update,
            req,
            setup::DummyToken(""),
            setup::DummyRefresh,
        )
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_unauthenticated_without_token_for_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;

    let req = blockjoy::HostUpdateRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        id: host.id.to_string(),
        ip: Some("123.456.789.0".into()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
        os: None,
        os_version: None,
        version: None,
    };
    let status = tester.send(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_unauthenticated_with_bad_token_for_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let host_id = host.id.to_string();

    let req = blockjoy::HostUpdateRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        id: host_id,
        version: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
    };
    let status = tester
        .send_with(
            Service::update,
            req,
            setup::DummyToken("923783"),
            setup::DummyRefresh,
        )
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_permission_denied_with_token_ownership_for_update() {
    let tester = setup::Tester::new().await;

    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);

    let other_host = tester.host2().await;
    let req = blockjoy::HostUpdateRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        id: other_host.id.to_string(),
        version: Some("3".to_string()),
        os: Some("LuukOS".to_string()),
        os_version: Some("5".to_string()),
        ip: Some("123.456.789.0".to_string()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
    };

    let status = tester
        .send_with(Service::update, req, token, refresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_not_found_for_wrong_otp() {
    let tester = setup::Tester::new().await;
    let req = blockjoy::ProvisionHostRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        otp: "unknown-otp".into(),
        status: blockjoy::ConnectionStatus::Online.into(),
        name: "tester".to_string(),
        version: "3".to_string(),
        ip: "123.456.789.0".to_string(),
        cpu_count: 2,
        mem_size: 2,
        disk_size: 2,
        os: "LuukOS".to_string(),
        os_version: "4".to_string(),
    };
    let status = tester.send(Service::provision, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_ok_for_provision() {
    let tester = setup::Tester::new().await;
    let from = "172.168.0.1".parse().unwrap();
    let to = "172.168.0.10".parse().unwrap();
    let gateway = "172.168.0.100".parse().unwrap();
    let mut conn = tester.conn().await;
    let host_provision = models::NewHostProvision::new(None, from, to, gateway)
        .unwrap()
        .create(&mut conn)
        .await
        .unwrap();
    let req = blockjoy::ProvisionHostRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        otp: host_provision.id,
        status: blockjoy::ConnectionStatus::Online.into(),
        name: "tester".to_string(),
        version: "3".to_string(),
        ip: "123.456.789.0".to_string(),
        cpu_count: 2,
        mem_size: 2,
        disk_size: 2,
        os: "LuukOS".to_string(),
        os_version: "4".to_string(),
    };
    tester.send(Service::provision, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let req = blockjoy::HostUpdateRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        id: host.id.to_string(),
        version: Some("3".to_string()),
        os: Some("LuukOS".to_string()),
        os_version: Some("5".to_string()),
        ip: Some("123.456.789.0".to_string()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
    };
    tester
        .send_with(Service::update, req, token, refresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_ok_for_delete() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let req = blockjoy::DeleteHostRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        host_id: host.id.to_string(),
    };
    tester
        .send_with(Service::delete, req, token, refresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_unauthenticated_without_token_for_delete() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let req = blockjoy::DeleteHostRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        host_id: host.id.to_string(),
    };
    let status = tester.send(Service::delete, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_permission_denied_for_delete() {
    let tester = setup::Tester::new().await;

    let host = tester.host().await;
    let req = blockjoy::DeleteHostRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        host_id: host.id.to_string(),
    };

    let other_host = tester.host2().await;
    // now we generate a token for the wrong host.
    let token = tester.host_token(&other_host);
    let refresh = tester.refresh_for(&token);

    let status = tester
        .send_with(Service::delete, req, token, refresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn can_update_host_info() {
    use models::schema::hosts;
    // TODO @Thomas: This doesn't really test the api, should this be here or maybe in
    // `src/models/host.rs`?

    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let update_host = models::UpdateHost {
        id: host.id,
        name: Some("tester"),
        ip_range_from: Some("192.168.0.10".parse().unwrap()),
        ip_range_to: Some("192.168.0.20".parse().unwrap()),
        ip_gateway: Some("192.168.0.1".parse().unwrap()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
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
