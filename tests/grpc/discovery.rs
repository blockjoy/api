use blockvisor_api::grpc::api;

type Service = api::discovery_client::DiscoveryClient<super::Channel>;

#[tokio::test]
async fn responds_unauthenticated_with_empty_token_fors_services() {
    let tester = super::Tester::new().await;
    let req = api::ServicesRequest {};
    let (token, refresh) = (super::DummyToken(""), super::DummyRefresh);
    let status = tester
        .send_with(Service::services, req, token, refresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_correct_urls_forss() {
    let tester = super::Tester::new().await;
    let req = api::ServicesRequest {};

    let response = tester.send_admin(Service::services, req).await.unwrap();

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
