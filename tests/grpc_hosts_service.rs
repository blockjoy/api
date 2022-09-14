#[allow(dead_code)]
mod setup;

use api::auth::TokenIdentifyable;
use api::grpc::blockjoy;
use api::grpc::blockjoy::{
    hosts_client::HostsClient, DeleteHostRequest, HostInfoUpdateRequest, ProvisionHostRequest,
};
use api::models::{Host, HostProvision, HostProvisionRequest, HostSelectiveUpdate};
use setup::{get_test_host, server_and_client_stub, setup};
use std::sync::Arc;
use test_macros::*;
use tonic::transport::Channel;
use tonic::{Request, Status};
use uuid::Uuid;

#[before(call = "setup")]
#[tokio::test]
async fn responds_unauthenticated_without_valid_token_for_info_update() {
    let db = Arc::new(_before_values.await);
    let hosts = Host::find_all(&db).await.unwrap();
    let host = hosts.first().unwrap();
    let b_uuid = blockjoy::Uuid::from(host.id);
    let host_info = blockjoy::HostInfo {
        id: Some(b_uuid.clone()),
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
    };
    let inner = HostInfoUpdateRequest {
        request_id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        info: Some(host_info),
    };
    let mut request = Request::new(inner);

    request
        .metadata_mut()
        .insert("authorization", "".parse().unwrap());

    assert_grpc_request! { info_update, request, tonic::Code::Unauthenticated, db, HostsClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_unauthenticated_without_token_for_info_update() {
    let db = Arc::new(_before_values.await);
    let hosts = Host::find_all(&db).await.unwrap();
    let host = hosts.first().unwrap();
    let b_uuid = blockjoy::Uuid::from(host.id);

    let host_info = blockjoy::HostInfo {
        id: Some(b_uuid.clone()),
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
    };
    let inner = HostInfoUpdateRequest {
        request_id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        info: Some(host_info),
    };

    assert_grpc_request! { info_update, Request::new(inner), tonic::Code::Unauthenticated, db, HostsClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_unauthenticated_with_token_for_info_update() {
    let db = Arc::new(_before_values.await);
    let host = get_test_host(&db).await;
    let b_uuid = blockjoy::Uuid::from(host.id);

    let host_info = blockjoy::HostInfo {
        id: Some(b_uuid.clone()),
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
    };
    let inner = HostInfoUpdateRequest {
        request_id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        info: Some(host_info),
    };
    let mut request = Request::new(inner);

    request
        .metadata_mut()
        .insert("authorization", "923783".parse().unwrap());

    assert_grpc_request! { info_update, request, tonic::Code::Unauthenticated, db, HostsClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_permission_denied_with_token_ownership_for_info_update() {
    let db = Arc::new(_before_values.await);
    let hosts = Host::find_all(&db).await.unwrap();
    let request_token = hosts.first().unwrap().get_token(&db).await.unwrap();
    let resource_host = hosts.last().unwrap();
    let b_uuid = blockjoy::Uuid::from(resource_host.id);
    let host_info = blockjoy::HostInfo {
        id: Some(b_uuid.clone()),
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
    };
    let inner = HostInfoUpdateRequest {
        request_id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        info: Some(host_info),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", request_token.to_base64())
            .parse()
            .unwrap(),
    );

    assert_grpc_request! { info_update, request, tonic::Code::PermissionDenied, db, HostsClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_not_found_for_provision() {
    let db = Arc::new(_before_values.await);
    let b_uuid = blockjoy::Uuid::from(Uuid::new_v4());
    let host_info = blockjoy::HostInfo {
        id: Some(b_uuid),
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
    };
    let inner = ProvisionHostRequest {
        request_id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        otp: "unknown-otp".into(),
        info: Some(host_info),
        validator_ips: vec![],
        org_id: None,
        status: 0,
    };
    let request = Request::new(inner);

    assert_grpc_request! { provision, request, tonic::Code::NotFound, db, HostsClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_provision() {
    let db = Arc::new(_before_values.await);
    let mut tx = db.begin().await.unwrap();
    let org: (Uuid,) = sqlx::query_as("select id from orgs")
        .fetch_one(&mut tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let host_provision_request = HostProvisionRequest {
        org_id: org.0,
        nodes: None,
    };
    let host_provision = HostProvision::create(host_provision_request, &db)
        .await
        .unwrap();
    let host_info = blockjoy::HostInfo {
        id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
    };
    let inner = ProvisionHostRequest {
        request_id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        otp: host_provision.id,
        info: Some(host_info),
        validator_ips: vec![],
        org_id: None,
        status: 0,
    };
    let request = Request::new(inner);

    assert_grpc_request! { provision, request, tonic::Code::Ok, db, HostsClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_info_update() {
    let db = Arc::new(_before_values.await);
    let hosts = Host::find_all(&db).await.unwrap();
    let host = hosts.first().unwrap();
    let token = host.get_token(&db).await.unwrap();
    let b_uuid = blockjoy::Uuid::from(host.id);
    let host_info = blockjoy::HostInfo {
        id: Some(b_uuid.clone()),
        name: Some("tester".into()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: Some("123.456.789.0".into()),
    };
    let inner = HostInfoUpdateRequest {
        request_id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        info: Some(host_info),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { info_update, request, tonic::Code::Ok, db, HostsClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_delete() {
    let db = Arc::new(_before_values.await);
    let hosts = Host::find_all(&db).await.unwrap();
    let host = hosts.first().unwrap();
    let token = host.get_token(&db).await.unwrap();
    let b_uuid = blockjoy::Uuid::from(host.id);
    let inner = DeleteHostRequest {
        request_id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        host_id: Some(b_uuid),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { delete, request, tonic::Code::Ok, db, HostsClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_unauthenticated_without_valid_token_for_delete() {
    let db = Arc::new(_before_values.await);
    let hosts = Host::find_all(&db).await.unwrap();
    let host = hosts.first().unwrap();
    let b_uuid = blockjoy::Uuid::from(host.id);
    let inner = DeleteHostRequest {
        request_id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        host_id: Some(b_uuid),
    };

    assert_grpc_request! { delete, Request::new(inner), tonic::Code::Unauthenticated, db, HostsClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_permission_denied_for_delete() {
    let db = Arc::new(_before_values.await);
    let hosts = Host::find_all(&db).await.unwrap();
    let host = hosts.first().unwrap();
    let request_host = hosts.last().unwrap();
    let token = request_host.get_token(&db).await.unwrap();
    let b_uuid = blockjoy::Uuid::from(host.id);
    let inner = DeleteHostRequest {
        request_id: Some(blockjoy::Uuid::from(Uuid::new_v4())),
        host_id: Some(b_uuid),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { delete, request, tonic::Code::PermissionDenied, db, HostsClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn can_update_host_info() {
    let db = Arc::new(_before_values.await);
    let host = get_test_host(&db).await;
    let host_info = blockjoy::HostInfo {
        id: Some(blockjoy::Uuid::from(host.id)),
        name: Some("tester".to_string()),
        version: None,
        location: None,
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip: None,
    };
    let fields = HostSelectiveUpdate::from(host_info);

    assert_eq!(
        Host::update_all(host.id, fields, &db).await.unwrap().name,
        "tester".to_string()
    );

    // Fetch host after update to see if it really worked as expected
    let mut tx = db.begin().await.unwrap();
    let row = sqlx::query(r#"SELECT * from hosts where ID = $1"#)
        .bind(host.id)
        .fetch_one(&mut tx)
        .await;
    tx.commit().await.unwrap();

    match row {
        Ok(row) => {
            let updated_host = Host::from(row);

            assert_eq!(updated_host.name, "tester".to_string());
            assert!(!updated_host.ip_addr.is_empty())
        }
        Err(e) => panic!("{:?}", e),
    }
}
