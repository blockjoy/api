use blockvisor_api::grpc::api;

use crate::setup::helper::traits::{DiscoveryService, SocketRpc};
use crate::setup::TestServer;

#[tokio::test]
async fn discovery_service_urls() {
    let test = TestServer::new().await;

    let req = api::DiscoveryServiceServicesRequest {};
    let resp = test
        .send_admin(DiscoveryService::services, req)
        .await
        .unwrap();

    let expected = test.context().config.mqtt.notification_url();
    assert_eq!(resp.notification_url, expected);
}
