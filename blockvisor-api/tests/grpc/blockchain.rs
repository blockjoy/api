use blockvisor_api::database::seed::{BLOCKCHAIN_ID, BLOCKCHAIN_NODE_TYPE, BLOCKCHAIN_VERSION};
use blockvisor_api::grpc::{api, common};
use blockvisor_api::models::NodeType;
use tonic::transport::Channel;
use uuid::Uuid;

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type Service = api::blockchain_service_client::BlockchainServiceClient<Channel>;

#[tokio::test]
async fn responds_ok_for_get_existing() {
    let test = TestServer::new().await;
    let org_id = test.org().await.id.to_string();
    let req = api::BlockchainServiceGetRequest {
        id: BLOCKCHAIN_ID.to_string(),
        org_id: Some(org_id),
    };
    test.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_not_found_for_get_nonexisting() {
    let test = TestServer::new().await;
    let org_id = test.org().await.id.to_string();
    let req = api::BlockchainServiceGetRequest {
        id: Uuid::new_v4().to_string(),
        org_id: Some(org_id),
    };
    let status = test.send_admin(Service::get, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_not_found_for_get_deleted() {
    let test = TestServer::new().await;
    let org_id = test.org().await.id.to_string();
    let req = api::BlockchainServiceGetRequest {
        id: Uuid::new_v4().to_string(),
        org_id: Some(org_id),
    };
    let status = test.send_admin(Service::get, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn can_list_blockchains() {
    let test = TestServer::new().await;
    let org_id = test.org().await.id.to_string();
    let req = api::BlockchainServiceListRequest {
        org_id: Some(org_id),
    };
    test.send_admin(Service::list, req).await.unwrap();
}

#[tokio::test]
async fn add_blockchain_node_type() {
    let test = TestServer::new().await;
    let request = |node_type: NodeType| api::BlockchainServiceAddNodeTypeRequest {
        id: BLOCKCHAIN_ID.to_string(),
        node_type: common::NodeType::from(node_type).into(),
        description: None,
    };

    // can't add a node type that already exists
    let req = request(BLOCKCHAIN_NODE_TYPE.parse().unwrap());
    let result = test.send_root(Service::add_node_type, req).await;
    assert_eq!(result.unwrap_err().code(), tonic::Code::AlreadyExists);

    // but can add a new node type
    let req = request(NodeType::Oracle);
    let result = test.send_root(Service::add_node_type, req).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn add_blockchain_version() {
    let test = TestServer::new().await;
    let request = |version: &str, node_type: NodeType| api::BlockchainServiceAddVersionRequest {
        id: BLOCKCHAIN_ID.to_string(),
        version: version.to_string(),
        description: None,
        node_type: common::NodeType::from(node_type).into(),
        properties: vec![],
    };

    let node_type = BLOCKCHAIN_NODE_TYPE.parse().unwrap();

    // can't add a version that already exists
    let req = request(BLOCKCHAIN_VERSION, node_type);
    let result = test.send_root(Service::add_version, req).await;
    assert_eq!(result.unwrap_err().code(), tonic::Code::AlreadyExists);

    // can't add a new version to a node type that doesn't exist
    let req = request("1.33.7", NodeType::Oracle);
    let result = test.send_root(Service::add_version, req).await;
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);

    // can add a new version to an existing node type
    let req = request("1.33.7", node_type);
    let result = test.send_root(Service::add_version, req).await;
    assert!(result.is_ok());
}
