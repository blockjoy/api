#[allow(dead_code)]
mod setup;

use api::grpc::blockjoy::{self, key_files_client, KeyFilesGetRequest};
use api::models;
use sqlx::types::Json;
use tonic::transport;
use uuid::Uuid;

type Service = key_files_client::KeyFilesClient<transport::Channel>;

#[tokio::test]
async fn responds_ok_with_invalid_node_id() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let req = KeyFilesGetRequest {
        request_id: None,
        node_id: Uuid::new_v4().to_string(),
    };
    tester
        .send_with(Service::get, req, auth, refresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_ok_with_valid_node_id() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let mut req = models::NodeCreateRequest {
        org_id: tester.org().await.id,
        blockchain_id: tester.blockchain().await.id,
        node_type: Json(models::NodeProperties::special_type(
            models::NodeTypeKey::Validator,
        )),
        chain_status: models::NodeChainStatus::Unknown,
        sync_status: models::NodeSyncStatus::Syncing,
        container_status: models::ContainerStatus::Installing,
        address: None,
        wallet_address: None,
        block_height: None,
        groups: None,
        node_data: None,
        ip_addr: None,
        ip_gateway: Some("192.168.0.1".into()),
        name: None,
        version: Some("3.3.0".into()),
        staking_status: None,
        self_update: false,
        vcpu_count: 0,
        mem_size_mb: 0,
        disk_size_gb: 0,
        host_name: "some host".to_string(),
        network: "some network".to_string(),
    };
    let mut tx = tester.begin().await;
    let node = models::Node::create(&mut req, &mut tx).await.unwrap();
    let req = models::CreateNodeKeyFileRequest {
        name: "my-key.txt".to_string(),
        content:
            "asödlfasdf asdfjaöskdjfalsdjföasjdf afa sdffasdfasldfjasödfj asdföalksdföalskdjfa"
                .to_string(),
        node_id: node.id,
    };
    models::NodeKeyFile::create(req, &mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let req = KeyFilesGetRequest {
        request_id: None,
        node_id: node.id.to_string(),
    };
    tester
        .send_with(Service::get, req, auth, refresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_not_found_with_invalid_node_id_for_save() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let key_file = blockjoy::Keyfile {
        name: "new keyfile".to_string(),
        content: "üöäß@niesfiefasd".to_string().into_bytes(),
    };
    let req = blockjoy::KeyFilesSaveRequest {
        request_id: None,
        node_id: Uuid::new_v4().to_string(),
        key_files: vec![key_file],
    };
    let status = tester
        .send_with(Service::save, req, auth, refresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_ok_with_valid_node_id_for_save() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let mut req = models::NodeCreateRequest {
        org_id: tester.org().await.id,
        blockchain_id: tester.blockchain().await.id,
        node_type: Json(models::NodeProperties::special_type(
            models::NodeTypeKey::Validator,
        )),
        chain_status: models::NodeChainStatus::Unknown,
        sync_status: models::NodeSyncStatus::Syncing,
        container_status: models::ContainerStatus::Installing,
        address: None,
        wallet_address: None,
        block_height: None,
        groups: None,
        node_data: None,
        ip_addr: None,
        ip_gateway: Some("192.168.0.1".into()),
        name: None,
        version: Some("3.3.0".into()),
        staking_status: None,
        self_update: false,
        vcpu_count: 0,
        mem_size_mb: 0,
        disk_size_gb: 0,
        host_name: "some host".to_string(),
        network: "some network".to_string(),
    };
    let mut tx = tester.begin().await;
    let node = models::Node::create(&mut req, &mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let key_file = blockjoy::Keyfile {
        name: "new keyfile".to_string(),
        content: "üöäß@niesfiefasd".to_string().into_bytes(),
    };
    let req = blockjoy::KeyFilesSaveRequest {
        request_id: None,
        node_id: node.id.to_string(),
        key_files: vec![key_file],
    };
    tester
        .send_with(Service::save, req, auth, refresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_error_with_same_node_id_name_twice_for_save() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let mut req = models::NodeCreateRequest {
        org_id: tester.org().await.id,
        blockchain_id: tester.blockchain().await.id,
        node_type: Json(models::NodeProperties::special_type(
            models::NodeTypeKey::Validator,
        )),
        chain_status: models::NodeChainStatus::Unknown,
        sync_status: models::NodeSyncStatus::Syncing,
        container_status: models::ContainerStatus::Installing,
        address: None,
        wallet_address: None,
        block_height: None,
        groups: None,
        node_data: None,
        ip_addr: None,
        ip_gateway: Some("192.168.0.1".into()),
        name: None,
        version: Some("3.3.0".into()),
        staking_status: None,
        self_update: false,
        vcpu_count: 0,
        mem_size_mb: 0,
        disk_size_gb: 0,
        host_name: "some host".to_string(),
        network: "some network".to_string(),
    };
    let mut tx = tester.begin().await;
    let node = models::Node::create(&mut req, &mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let key_file = blockjoy::Keyfile {
        name: "new keyfile".to_string(),
        content: "üöäß@niesfiefasd".to_string().into_bytes(),
    };
    let req = blockjoy::KeyFilesSaveRequest {
        request_id: None,
        node_id: node.id.to_string(),
        key_files: vec![key_file.clone()],
    };
    let (auth_, refresh_) = (auth.clone(), refresh.clone());
    tester
        .send_with(Service::save, req, auth_, refresh_)
        .await
        .unwrap();

    let req = blockjoy::KeyFilesSaveRequest {
        request_id: None,
        node_id: node.id.to_string(),
        key_files: vec![key_file],
    };
    let status = tester
        .send_with(Service::save, req, auth, refresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument)
}
