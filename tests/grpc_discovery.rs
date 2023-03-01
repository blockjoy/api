mod setup;

use api::grpc::blockjoy::{discovery_client, ServicesResponse};
use tonic::transport;
use tonic::Request;

type Service = discovery_client::DiscoveryClient<transport::Channel>;

#[tokio::test]
async fn responds_unauthenticated_with_empty_token_for_services() {
    let tester = setup::Tester::new().await;
    let req = Request::new(());
    let status = tester
        .send_with(
            Service::services,
            req,
            setup::DummyToken(""),
            setup::DummyRefresh,
        )
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_correct_urls_for_services() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let req = Request::new(());

    let response: ServicesResponse = tester
        .send_with(Service::services, req, token, refresh)
        .await
        .unwrap();

    assert_eq!(
        response.key_service_url,
        std::env::var("KEY_SERVICE_URL").unwrap()
    );
    assert_eq!(
        response.registry_url,
        std::env::var("COOKBOOK_URL").unwrap()
    );
    assert_eq!(
        response.notification_url,
        format!(
            "{}:{}",
            std::env::var("MQTT_SERVER_ADDRESS").unwrap(),
            std::env::var("MQTT_SERVER_PORT").unwrap()
        )
    );
}
