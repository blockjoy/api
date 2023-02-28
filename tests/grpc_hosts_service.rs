mod setup;

use api::grpc::blockjoy::{self, hosts_client};
use api::models;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use tonic::transport;

type Service = hosts_client::HostsClient<transport::Channel>;

#[tokio::test]
async fn responds_unauthenticated_with_empty_token_for_info_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let host_id = host.id.to_string();
    let host_info = blockjoy::HostInfo {
        id: Some(host_id.clone()),
        name: Some("tester".into()),
        ip: Some("123.456.789.0".into()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
        ..Default::default()
    };
    let req = blockjoy::HostInfoUpdateRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        info: Some(host_info),
    };
    let status = tester
        .send_with(
            Service::info_update,
            req,
            setup::DummyToken(""),
            setup::DummyRefresh,
        )
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_unauthenticated_without_token_for_info_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let host_id = host.id.to_string();

    let host_info = blockjoy::HostInfo {
        id: Some(host_id),
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
    };
    let req = blockjoy::HostInfoUpdateRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        info: Some(host_info),
    };
    let status = tester.send(Service::info_update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_unauthenticated_with_bad_token_for_info_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let host_id = host.id.to_string();

    let host_info = blockjoy::HostInfo {
        id: Some(host_id),
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
    };
    let req = blockjoy::HostInfoUpdateRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        info: Some(host_info),
    };
    let status = tester
        .send_with(
            Service::info_update,
            req,
            setup::DummyToken("923783"),
            setup::DummyRefresh,
        )
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_permission_denied_with_token_ownership_for_info_update() {
    let tester = setup::Tester::new().await;

    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);

    let other_host = tester.host2().await;
    let host_info = blockjoy::HostInfo {
        id: Some(other_host.id.to_string()),
        name: Some("tester".into()),
        ip: Some("123.456.789.0".into()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
        ..Default::default()
    };
    let req = blockjoy::HostInfoUpdateRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        info: Some(host_info),
    };

    let status = tester
        .send_with(Service::info_update, req, token, refresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_not_found_for_provision() {
    let tester = setup::Tester::new().await;
    let random_uuid = uuid::Uuid::new_v4().to_string();
    let host_info = blockjoy::HostInfo {
        id: Some(random_uuid), // does not exist
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
    };
    let req = blockjoy::ProvisionHostRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        otp: "unknown-otp".into(),
        info: Some(host_info),
        status: 0,
    };
    let status = tester.send(Service::provision, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_ok_for_provision() {
    let tester = setup::Tester::new().await;
    let create_host_provision = models::NewHostProvision::new(
        None,
        "172.168.0.1".parse().unwrap(),
        "172.168.0.10".parse().unwrap(),
        "172.168.0.100".parse().unwrap(),
    )
    .unwrap();
    let mut conn = tester.conn().await;
    let host_provision = create_host_provision.create(&mut conn).await.unwrap();
    let host_info = blockjoy::HostInfo {
        id: Some(uuid::Uuid::new_v4().to_string()),
        name: Some("tester".into()),
        ip: Some("123.456.789.0".into()),
        ip_gateway: Some("127.18.0.1".into()),
        ip_range_from: Some("127.18.0.10".into()),
        ip_range_to: Some("127.18.0.20".into()),
        ..Default::default()
    };
    let req = blockjoy::ProvisionHostRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        otp: host_provision.id,
        info: Some(host_info),
        status: 0,
    };
    tester.send(Service::provision, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_info_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let host_info = blockjoy::HostInfo {
        id: Some(host.id.to_string()),
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
        ip_gateway: Some("192.168.0.1".into()),
        ip_range_from: Some("192.168.0.10".into()),
        ip_range_to: Some("192.168.0.20".into()),
    };
    let req = blockjoy::HostInfoUpdateRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        info: Some(host_info),
    };
    tester
        .send_with(Service::info_update, req, token, refresh)
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
