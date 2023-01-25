mod setup;

use api::models::{
    ContainerStatus, CreateNodeKeyFileRequest, Node, NodeChainStatus, NodeCreateRequest,
    NodeKeyFile, NodeProperties, NodeSyncStatus, NodeTypeKey,
};
use sqlx::types::Json;

#[tokio::test]
async fn can_create_key_file() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.admin_user().await;
    let org = tester.org_for(&user).await;
    let mut req = NodeCreateRequest {
        org_id: org.id,
        blockchain_id: blockchain.id,
        node_type: Json(NodeProperties::special_type(NodeTypeKey::Validator)),
        chain_status: NodeChainStatus::Unknown,
        sync_status: NodeSyncStatus::Syncing,
        container_status: ContainerStatus::Installing,
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
    let node = Node::create(&mut req, &mut tx).await.unwrap();
    let req = CreateNodeKeyFileRequest {
        name: "my-key.txt".to_string(),
        content:
            "asödlfasdf asdfjaöskdjfalsdjföasjdf afa sdffasdfasldfjasödfj asdföalksdföalskdjfa"
                .to_string(),
        node_id: node.id,
    };
    let file = NodeKeyFile::create(req, &mut tx).await?;
    tx.commit().await.unwrap();

    assert_eq!(file.name(), "my-key.txt");

    Ok(())
}

#[tokio::test]
async fn cannot_create_key_file_for_unknown_node() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let req = CreateNodeKeyFileRequest {
        name: "my-key.txt".to_string(),
        content:
            "asödlfasdf asdfjaöskdjfalsdjföasjdf afa sdffasdfasldfjasödfj asdföalksdföalskdjfa"
                .to_string(),
        node_id: uuid::Uuid::new_v4(),
    };

    let mut tx = tester.begin().await;
    NodeKeyFile::create(req, &mut tx).await.unwrap_err();
    tx.commit().await.unwrap();
    Ok(())
}

#[tokio::test]
async fn deletes_key_file_if_node_is_deleted() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.admin_user().await;
    let org = tester.org_for(&user).await;
    let mut req = NodeCreateRequest {
        org_id: org.id,
        blockchain_id: blockchain.id,
        node_type: Json(NodeProperties::special_type(NodeTypeKey::Validator)),
        chain_status: NodeChainStatus::Unknown,
        sync_status: NodeSyncStatus::Syncing,
        container_status: ContainerStatus::Installing,
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
    let node = Node::create(&mut req, &mut tx).await.unwrap();
    let req = CreateNodeKeyFileRequest {
        name: "my-key.txt".to_string(),
        content:
            "asödlfasdf asdfjaöskdjfalsdjföasjdf afa sdffasdfasldfjasödfj asdföalksdföalskdjfa"
                .to_string(),
        node_id: node.id,
    };
    let file = NodeKeyFile::create(req, &mut tx).await?;

    assert_eq!(file.name(), "my-key.txt");

    Node::delete(node.id, &mut tx).await?;

    let cnt: i32 = sqlx::query_scalar("select count(*)::int from node_key_files")
        .fetch_one(&mut tx)
        .await?;
    tx.commit().await.unwrap();

    assert_eq!(cnt, 0);

    Ok(())
}
