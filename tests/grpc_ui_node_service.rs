mod setup;

use api::grpc::blockjoy_ui::{self, node_service_client};
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
    let user = tester.admin_user().await;
    let org = tester.org_for(&user).await;
    let req = blockjoy_ui::CreateNodeRequest {
        meta: Some(tester.meta()),
        org_id: org.id.to_string(),
        blockchain_id: blockchain.id.to_string(),
        r#type: blockjoy_ui::node::NodeType::Validator.into(),
        properties: vec![],
        version: Some("3.3.0".into()),
        network: "some network".to_string(),
    };
    tester.send_admin(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_invalid_argument_with_invalid_data_for_create() {
    let tester = setup::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let req = blockjoy_ui::CreateNodeRequest {
        meta: Some(tester.meta()),
        // This is an invalid uuid so the api call should fail.
        org_id: "wowowowowow".to_string(),
        blockchain_id: blockchain.id.to_string(),
        r#type: blockjoy_ui::node::NodeType::Api.into(),
        properties: vec![],
        version: Some("3.3.0".into()),
        network: "some network".to_string(),
    };
    let status = tester.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_update() {
    let tester = setup::Tester::new().await;
    let node = tester.node().await;
    let req = blockjoy_ui::UpdateNodeRequest {
        meta: Some(tester.meta()),
        id: node.id.to_string(),
        version: Some("10".to_string()),
    };
    tester.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_with_invalid_data_for_update() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::UpdateNodeRequest {
        meta: Some(tester.meta()),
        // This is an invalid uuid so the api call should fail.
        id: "wowowow".to_string(),
        version: Some("stri-bu".to_string()),
    };
    let status = tester.send_admin(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_not_found_with_invalid_id_for_update() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::UpdateNodeRequest {
        meta: Some(tester.meta()),
        // This uuid will not exist, so the api call should fail.
        id: uuid::Uuid::new_v4().to_string(),
        version: Some("stri-bu".to_string()),
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
