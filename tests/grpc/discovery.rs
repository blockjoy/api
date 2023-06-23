use blockvisor_api::grpc::api;

type Service = api::discovery_service_client::DiscoveryServiceClient<super::Channel>;

#[tokio::test]
async fn responds_correct_urls_forss() {
    let tester = super::Tester::new().await;
    let req = api::DiscoveryServiceServicesRequest {};

    let response = tester.send_admin(Service::services, req).await.unwrap();

    assert_eq!(
        response.key_service_url,
        tester.context().config.key_service.url.to_string()
    );
    assert_eq!(
        response.notification_url,
        tester.context().config.mqtt.notification_url()
    );
}
