use api::auth::FindableById;
use api::grpc::blockjoy;
use api::grpc::blockjoy::nodes_client::NodesClient;
use api::models::Node;
use tonic::transport::Channel;
use uuid::Uuid;

mod setup;

type Service = NodesClient<Channel>;

#[tokio::test]
async fn responds_ok_for_info_update() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let node = tester.node().await;
    let node_id = node.id.to_string();
    let block_height = 12123;
    let node_info = blockjoy::NodeInfo {
        id: node_id.clone(),
        host_id: Some(node.host_id.to_string()),
        name: None,
        ip: None,
        self_update: None,
        block_height: Some(block_height),
        onchain_name: None,
        app_status: None,
        container_status: None,
        sync_status: None,
        staking_status: None,
        address: None,
    };
    let req = blockjoy::NodeInfoUpdateRequest {
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        info: Some(node_info),
    };

    tester
        .send_with(Service::info_update, req, token, refresh)
        .await
        .unwrap();

    let mut conn = tester.conn().await;
    let node = Node::find_by_id(Uuid::parse_str(node_id.as_str()).unwrap(), &mut conn)
        .await
        .unwrap();

    assert_eq!(node.block_height.unwrap(), block_height);
}
