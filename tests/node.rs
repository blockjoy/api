mod setup;

use api::models::{self};

#[tokio::test]
async fn can_filter_nodes() -> anyhow::Result<()> {
    let mut name = String::from("test_");
    name.push_str(petname::petname(3, "_").as_str());

    let tester = setup::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.admin_user().await;
    let org = tester.org_for(&user).await;
    let req = models::NewNode {
        id: uuid::Uuid::new_v4(),
        org_id: org.id,
        blockchain_id: blockchain.id,
        properties: serde_json::to_value(models::NodeProperties {
            version: None,
            properties: Some(vec![]),
        })?,
        chain_status: models::NodeChainStatus::Unknown,
        sync_status: models::NodeSyncStatus::Syncing,
        container_status: models::ContainerStatus::Installing,
        block_height: None,
        groups: "".to_string(),
        node_data: None,
        name,
        version: Some("3.3.0"),
        staking_status: models::NodeStakingStatus::Staked,
        self_update: false,
        vcpu_count: 0,
        mem_size_mb: 0,
        disk_size_gb: 0,
        network: "some network",
        node_type: models::NodeType::Validator,
        created_by: user.id,
    };

    let mut conn = tester.conn().await;
    req.create(&mut conn).await.unwrap();

    let filter = models::NodeFilter {
        status: vec![models::NodeChainStatus::Unknown],
        node_types: vec![],
        blockchains: vec![blockchain.id],
    };

    let nodes = models::Node::find_all_by_filter(org.id, filter, 0, 10, &mut conn).await?;

    assert!(!nodes.is_empty());
    assert_eq!(nodes.len(), 1);

    Ok(())
}

#[tokio::test]
async fn has_dns_entry() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let node = tester.node().await;

    assert!(node.dns_record_id.is_some());

    Ok(())
}
