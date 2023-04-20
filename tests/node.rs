mod setup;

use blockvisor_api::models;

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
        version: "3.3.0",
        staking_status: models::NodeStakingStatus::Staked,
        self_update: false,
        vcpu_count: 0,
        mem_size_bytes: 0,
        disk_size_bytes: 0,
        network: "some network",
        node_type: models::NodeType::Validator,
        created_by: user.id,
        scheduler_similarity: None,
        scheduler_resource: models::ResourceAffinity::MostResources,
    };

    let mut conn = tester.conn().await;
    req.create(&mut conn).await.unwrap();

    let filter = models::NodeFilter {
        status: vec![models::NodeChainStatus::Unknown],
        node_types: vec![],
        blockchains: vec![blockchain.id],
        limit: 10,
        offset: 0,
        org_id: org.id,
    };

    let nodes = models::Node::filter(filter, &mut conn).await?;

    assert_eq!(nodes.len(), 1);

    Ok(())
}

// #[tokio::test]
// async fn has_dns_entry() {
//     let mut conn = tester.conn().await;
//     let tester = setup::Tester::new().await;
//     let node = models::NewNode {
//         id: uuid::Uuid::new_v4(),
//         org_id: tester.org().await.id,
//         name: "noderoni".to_string(),
//         groups: "".to_string(),
//         version: Some("latest"),
//         blockchain_id: tester.blockchain().await.id,
//         properties: serde_json::Value::default(),
//         address: None,
//         wallet_address: None,
//         block_height: None,
//         node_data: None,
//         chain_status: models::NodeChainStatus::Broadcasting,
//         sync_status: models::NodeSyncStatus::Synced,
//         staking_status: models::NodeStakingStatus::Unstaked,
//         container_status: models::ContainerStatus::Snapshotting,
//         self_update: true,
//         vcpu_count: 2,
//         mem_size_mb: 3,
//         disk_size_gb: 4,
//         network: "goerli",
//         node_type: models::NodeType::Validator,
//         created_by: tester.admin_user().await.id,
//     }
//     .create(&mut conn)
//     .await
//     .unwrap();

//     assert!(node.dns_record_id.is_some());
// }
