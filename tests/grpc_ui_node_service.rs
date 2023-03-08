mod setup;

use api::grpc::blockjoy_ui::{self, node, node_service_client};
use api::models;
use tonic::transport;

type Service = node_service_client::NodeServiceClient<transport::Channel>;

#[tokio::test]
async fn responds_not_found_without_any_for_get() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::GetNodeRequest {
        meta: Some(tester.meta()),
        id: uuid::Uuid::new_v4().to_string(),
    };
    let status = tester.send_admin(Service::get, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_ok_with_id_for_get() {
    let tester = setup::Tester::new().await;
    let node = tester.node().await;
    let req = blockjoy_ui::GetNodeRequest {
        meta: Some(tester.meta()),
        id: node.id.to_string(),
    };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_create() {
    let tester = setup::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let host = tester.host().await;
    let user = tester.admin_user().await;
    let org = tester.org_for(&user).await;
    let node = blockjoy_ui::Node {
        id: None,
        host_id: Some(host.id.to_string()),
        org_id: Some(org.id.to_string()),
        blockchain_id: Some(blockchain.id.to_string()),
        status: Some(node::NodeStatus::UndefinedApplicationStatus as i32),
        r#type: Some(
            models::NodeType::special_type(models::NodeTypeKey::Validator)
                .to_json()
                .unwrap(),
        ),
        ip_gateway: Some("192.168.0.1".into()),
        groups: vec![],
        staking_status: None,
        sync_status: Some(models::NodeSyncStatus::Unknown as i32),
        self_update: None,
        version: Some("3.3.0".into()),
        network: Some("some network".to_string()),
        ..Default::default()
    };
    let req = blockjoy_ui::CreateNodeRequest {
        meta: Some(tester.meta()),
        node: Some(node),
    };
    tester.send_admin(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_invalid_argument_with_invalid_data_for_create() {
    let tester = setup::Tester::new().await;
    let node = blockjoy_ui::Node {
        // This is required so the test should fail:
        org_id: None,
        status: Some(node::NodeStatus::UndefinedApplicationStatus as i32),
        r#type: Some(
            models::NodeType::special_type(models::NodeTypeKey::Api)
                .to_json()
                .unwrap(),
        ),
        sync_status: Some(models::NodeSyncStatus::Unknown as i32),
        ..Default::default()
    };
    let req = blockjoy_ui::CreateNodeRequest {
        meta: Some(tester.meta()),
        node: Some(node),
    };
    let status = tester.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_update() {
    let tester = setup::Tester::new().await;
    let node = tester.node().await;
    let node = blockjoy_ui::Node {
        id: Some(node.id.to_string()),
        name: Some("stri-bu".to_string()),
        ..Default::default()
    };
    let req = blockjoy_ui::UpdateNodeRequest {
        meta: Some(tester.meta()),
        node: Some(node),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_with_invalid_data_for_update() {
    let tester = setup::Tester::new().await;
    let node = blockjoy_ui::Node {
        // This should cause an error
        id: None,
        name: Some("stri-bu".to_string()),
        ..Default::default()
    };
    let req = blockjoy_ui::UpdateNodeRequest {
        meta: Some(tester.meta()),
        node: Some(node),
    };
    let status = tester.send_admin(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_not_found_with_invalid_id_for_update() {
    let tester = setup::Tester::new().await;
    let node = blockjoy_ui::Node {
        // This should cause an error
        id: Some(uuid::Uuid::new_v4().to_string()),
        name: Some("stri-bu".to_string()),
        ..Default::default()
    };
    let req = blockjoy_ui::UpdateNodeRequest {
        meta: Some(tester.meta()),
        node: Some(node),
    };
    let status = tester.send_admin(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound, "{status:?}");
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_delete() {
    let tester = setup::Tester::new().await;
    let node = tester.node().await;
    let req = blockjoy_ui::DeleteNodeRequest {
        meta: Some(tester.meta()),
        id: node.id.to_string(),
    };
    tester.send_admin(Service::delete, req).await.unwrap();
}
