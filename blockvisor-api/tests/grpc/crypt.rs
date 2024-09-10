use blockvisor_api::auth::resource::{NodeId, Resource};
use blockvisor_api::grpc::{api, common};
use tonic::Code;
use uuid::Uuid;

use crate::setup::helper::traits::{CryptService, NodeService, SocketRpc};
use crate::setup::TestServer;
use crate::test_name;

const TEST_SECRET: &[u8] = b"super secret stuff";

#[tokio::test]
async fn node_can_create_secrets() {
    let test = TestServer::new().await;
    let jwt = test.node_jwt();

    // can't create a secret for another resource
    let req = api::CryptServicePutSecretRequest {
        resource: Some(common::Resource::from(Resource::Host(test.seed().host1.id))),
        name: test_name!().into(),
        value: TEST_SECRET.to_vec(),
    };
    let status = test
        .send_with(CryptService::put_secret, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);

    // can't create a secret for an unknown node id
    let req = api::CryptServicePutSecretRequest {
        resource: Some(common::Resource::from(Resource::Node(
            Uuid::new_v4().into(),
        ))),
        name: test_name!().into(),
        value: TEST_SECRET.to_vec(),
    };
    let status = test
        .send_with(CryptService::put_secret, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::NotFound);

    // can create a secret for the token's node id
    let req = api::CryptServicePutSecretRequest {
        resource: Some(common::Resource::from(Resource::Node(test.seed().node.id))),
        name: test_name!().into(),
        value: TEST_SECRET.to_vec(),
    };
    test.send_with(CryptService::put_secret, req, &jwt)
        .await
        .unwrap();
}

#[tokio::test]
async fn node_can_read_secrets() {
    let test = TestServer::new().await;
    let jwt = test.node_jwt();

    let req = api::CryptServicePutSecretRequest {
        resource: Some(common::Resource::from(Resource::Node(test.seed().node.id))),
        name: test_name!().into(),
        value: TEST_SECRET.to_vec(),
    };
    test.send_with(CryptService::put_secret, req, &jwt)
        .await
        .unwrap();

    let req = api::CryptServiceGetSecretRequest {
        resource: Some(common::Resource::from(Resource::Node(test.seed().node.id))),
        name: test_name!().into(),
    };
    let secret = test
        .send_with(CryptService::get_secret, req, &jwt)
        .await
        .unwrap();
    assert_eq!(secret.value, TEST_SECRET);
}

#[tokio::test]
async fn delete_node_deletes_secrets() {
    let test = TestServer::new().await;

    let req = create_node(&test, None);
    let resp = test.send_admin(NodeService::create, req).await.unwrap();
    assert_eq!(resp.nodes.len(), 1);

    let node = resp.nodes[0].clone();
    let node_id: NodeId = node.node_id.parse().unwrap();
    let claims = test.node_claims_for(node_id);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let req = api::CryptServicePutSecretRequest {
        resource: Some(common::Resource::from(Resource::Node(node_id))),
        name: test_name!().into(),
        value: TEST_SECRET.to_vec(),
    };
    test.send_with(CryptService::put_secret, req, &jwt)
        .await
        .unwrap();

    let req = api::CryptServiceGetSecretRequest {
        resource: Some(common::Resource::from(Resource::Node(node_id))),
        name: test_name!().into(),
    };
    let secret = test
        .send_with(CryptService::get_secret, req, &jwt)
        .await
        .unwrap();
    assert_eq!(secret.value, TEST_SECRET);

    let req = api::NodeServiceDeleteRequest {
        node_id: node.node_id.to_string(),
    };
    test.send_admin(NodeService::delete, req).await.unwrap();

    let req = api::CryptServiceGetSecretRequest {
        resource: Some(common::Resource::from(Resource::Node(node_id))),
        name: test_name!().into(),
    };
    let status = test
        .send_with(CryptService::get_secret, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::NotFound);
}

#[tokio::test]
async fn new_node_with_old_id_copies_secrets() {
    let test = TestServer::new().await;

    let req = create_node(&test, None);
    let resp = test.send_admin(NodeService::create, req).await.unwrap();
    assert_eq!(resp.nodes.len(), 1);

    let node = resp.nodes[0].clone();
    let node_id: NodeId = node.node_id.parse().unwrap();
    let claims = test.node_claims_for(node_id);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let put_secret = |name: &str| api::CryptServicePutSecretRequest {
        resource: Some(common::Resource::from(Resource::Node(node_id))),
        name: name.to_string(),
        value: TEST_SECRET.to_vec(),
    };

    let req = put_secret("keep-me");
    test.send_with(CryptService::put_secret, req, &jwt)
        .await
        .unwrap();

    let req = put_secret("delete-me");
    test.send_with(CryptService::put_secret, req, &jwt)
        .await
        .unwrap();

    let path = format!("node/{node_id}/secret/delete-me");
    test.context()
        .vault
        .read()
        .await
        .delete_path(&path)
        .await
        .unwrap();

    let req = create_node(&test, Some(node_id));
    let resp = test.send_admin(NodeService::create, req).await.unwrap();
    assert_eq!(resp.nodes.len(), 1);
    let new_node = resp.nodes[0].clone();
    let new_node_id: NodeId = new_node.node_id.parse().unwrap();

    let prefix = format!("node/{new_node_id}/secret");
    let mut names = test
        .context()
        .vault
        .read()
        .await
        .list_path(&prefix)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(names.len(), 1);
    assert_eq!(names.pop().unwrap(), "keep-me");
}

fn create_node(test: &TestServer, old_node_id: Option<NodeId>) -> api::NodeServiceCreateRequest {
    api::NodeServiceCreateRequest {
        org_id: test.seed().org.id.to_string(),
        image_id: test.seed().image.id.to_string(),
        old_node_id: old_node_id.map(|id| id.to_string()),
        placement: Some(common::NodePlacement {
            placement: Some(common::node_placement::Placement::HostId(
                test.seed().host1.id.to_string(),
            )),
        }),
        new_values: vec![],
        add_rules: vec![],
        tags: None,
    }
}
