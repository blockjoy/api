use blockvisor_api::grpc::api;
use tonic::transport::Channel;

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type Service = api::discovery_service_client::DiscoveryServiceClient<Channel>;

#[tokio::test]
async fn responds_correct_urls_forss() {
    let test = TestServer::new().await;
    let req = api::DiscoveryServiceServicesRequest {};

    let response = test.send_admin(Service::services, req).await.unwrap();

    assert_eq!(
        response.key_service_url,
        test.context().config.key_service.url.to_string()
    );
    assert_eq!(
        response.notification_url,
        test.context().config.mqtt.notification_url()
    );
}
