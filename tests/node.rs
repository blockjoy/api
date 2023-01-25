mod setup;

use api::models::{
    ContainerStatus, Node, NodeChainStatus, NodeCreateRequest, NodeFilter, NodeProperties,
    NodeSyncStatus, NodeTypeKey,
};
use sqlx::types::Json;

#[tokio::test]
async fn can_filter_nodes() -> anyhow::Result<()> {
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
    Node::create(&mut req, &mut tx).await.unwrap();

    let filter = NodeFilter {
        status: vec!["unknown".to_string()],
        node_types: vec![],
        blockchains: vec![blockchain.id],
    };

    let nodes = Node::find_all_by_filter(org.id, filter, 0, 10, &mut tx).await?;
    tx.commit().await.unwrap();

    assert!(!nodes.is_empty());
    assert_eq!(nodes.len(), 1);

    Ok(())
}
