use blockvisor_api::grpc::api;

type Service = api::discovery_service_client::DiscoveryServiceClient<super::Channel>;

#[tokio::test]
async fn responds_correct_urls_forss() {
    let tester = super::Tester::new().await;
    let req = api::DiscoveryServiceServicesRequest {};

    let response = tester.send_admin(Service::services, req).await.unwrap();

    assert_eq!(
        response.key_service_url,
        std::env::var("KEY_SERVICE_URL").unwrap()
    );
    assert_eq!(
        response.notification_url,
        format!(
            "mqtt://{}:{}",
            std::env::var("MQTT_SERVER_ADDRESS").unwrap(),
            std::env::var("MQTT_SERVER_PORT").unwrap()
        )
    );
}
