use api::auth::FindableById;
use api::grpc::blockjoy::{self, node_service_client};
use api::models::Node;
use tonic::transport::Channel;

mod setup;

type Service = node_service_client::NodeServiceClient<Channel>;

#[tokio::test]
async fn responds_ok_for_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let node = tester.node().await;
    let node_id = node.id.to_string();
    let req = blockjoy::NodeUpdateRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        id: node_id.clone(),
        ip: None,
        self_update: Some(true),
        container_status: None,
        address: None,
    };

    tester
        .send_with(Service::update, req, token, refresh)
        .await
        .unwrap();

    let mut conn = tester.conn().await;
    let node = Node::find_by_id(node_id.parse().unwrap(), &mut conn)
        .await
        .unwrap();

    assert!(node.self_update);
}
