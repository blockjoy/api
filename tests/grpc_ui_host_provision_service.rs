mod setup;

use api::grpc::blockjoy_ui::{self, host_provision_service_client};
use api::models;
use tonic::transport;

type Service = host_provision_service_client::HostProvisionServiceClient<transport::Channel>;

#[tokio::test]
async fn responds_not_found_without_valid_id_for_get() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::GetHostProvisionRequest {
        meta: Some(tester.meta()),
        id: Some("foo-bar1".to_string()),
    };
    let status = tester.send_admin(Service::get, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_ok_with_valid_id_for_get() {
    let tester = setup::Tester::new().await;
    let new_prov = models::NewHostProvision::new(
        None,
        "192.168.0.1".parse().unwrap(),
        "192.168.0.10".parse().unwrap(),
        "192.168.0.20".parse().unwrap(),
    )
    .unwrap();
    let mut conn = tester.conn().await;
    let provision = new_prov.create(&mut conn).await.unwrap();
    let req = blockjoy_ui::GetHostProvisionRequest {
        meta: Some(tester.meta()),
        id: Some(provision.id),
    };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_error_with_invalid_provision_for_create() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::CreateHostProvisionRequest {
        meta: Some(tester.meta()),
        ip_range_from: "192.168.0.1".to_string(),
        ip_range_to: "192.168.0.10".to_string(),
        ip_gateway: "192.168.0.1000".to_string(),
    };
    let status = tester.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_ok_with_valid_provision_for_create() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::CreateHostProvisionRequest {
        meta: Some(tester.meta()),
        ip_range_from: "192.168.0.1".to_string(),
        ip_range_to: "192.168.0.10".to_string(),
        ip_gateway: "192.168.0.20".to_string(),
    };
    tester.send_admin(Service::create, req).await.unwrap();
}
