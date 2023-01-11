mod setup;

use api::{
    auth::JwtToken,
    grpc::blockjoy_ui::{self, host_service_client},
};
use tonic::transport;

type Service = host_service_client::HostServiceClient<transport::Channel>;

#[tokio::test]
async fn responds_invalid_argument_without_any_for_get() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::GetHostsRequest {
        meta: Some(tester.meta()),
        param: None,
    };
    let status = tester.send_admin(Service::get, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_ok_with_host_id_for_get() {
    let tester = setup::Tester::new().await;
    let host_id = tester.host().await.id.to_string();
    let req = blockjoy_ui::GetHostsRequest {
        meta: Some(tester.meta()),
        param: Some(blockjoy_ui::get_hosts_request::Param::Id(host_id)),
    };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_with_token_for_get() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let host_token = tester.host_token(&host);
    let host_token = host_token.encode().unwrap();
    let req = blockjoy_ui::GetHostsRequest {
        meta: Some(tester.meta()),
        param: Some(blockjoy_ui::get_hosts_request::Param::Token(host_token)),
    };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_with_id_for_delete() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let req = blockjoy_ui::DeleteHostRequest {
        meta: Some(tester.meta()),
        id: host.id.to_string(),
    };
    tester.send_admin(Service::delete, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_with_host_for_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let req = blockjoy_ui::UpdateHostRequest {
        meta: Some(tester.meta()),
        host: Some(host.try_into().unwrap()),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_with_host_for_create() {
    let tester = setup::Tester::new().await;
    let host = blockjoy_ui::Host {
        name: Some("burli-bua".to_string()),
        ip: Some("127.0.0.1".to_string()),
        ip_gateway: Some("128.168.0.1".to_string()),
        ip_range_from: Some("128.168.0.10".to_string()),
        ip_range_to: Some("128.168.0.100".to_string()),
        ..Default::default()
    };
    let req = blockjoy_ui::CreateHostRequest {
        meta: Some(tester.meta()),
        host: Some(host),
    };
    tester.send_admin(Service::create, req).await.unwrap();
}
