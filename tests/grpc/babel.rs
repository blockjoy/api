use blockvisor_api::auth::resource::{OrgId, UserId};
use blockvisor_api::grpc::api;
use blockvisor_api::models::blockchain::BlockchainId;
use blockvisor_api::models::node::NewNode;
use blockvisor_api::models::{
    ContainerStatus, NodeChainStatus, NodeStakingStatus, NodeSyncStatus, NodeType, ResourceAffinity,
};
use futures_util::{stream, StreamExt};
use uuid::Uuid;

type Service = api::babel_service_client::BabelServiceClient<super::Channel>;

fn create_new_node<'a>(
    index: usize,
    org_id: OrgId,
    blockchain_id: BlockchainId,
    user_id: UserId,
    version: &'a str,
    node_type: NodeType,
) -> NewNode<'a> {
    let id = Uuid::new_v4();
    let name = format!("node-{index}-{id}");
    NewNode {
        id: id.into(),
        org_id,
        blockchain_id: blockchain_id,
        chain_status: NodeChainStatus::Unknown,
        sync_status: NodeSyncStatus::Syncing,
        container_status: ContainerStatus::Installing,
        block_height: None,
        node_data: None,
        name,
        version,
        staking_status: NodeStakingStatus::Staked,
        self_update: true,
        vcpu_count: 0,
        mem_size_bytes: 0,
        disk_size_bytes: 0,
        network: "some network",
        node_type,
        created_by: user_id,
        scheduler_similarity: None,
        scheduler_resource: Some(ResourceAffinity::MostResources),
        scheduler_region: None,
        allow_ips: serde_json::json!([]),
        deny_ips: serde_json::json!([]),
    }
}

#[tokio::test]
#[ignore]
async fn test_notify_success() {
    let tester = &super::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.user().await;
    let org = tester.org_for(&user).await;
    let host_id = tester.host().await.id;
    let ip_address = tester.host().await.ip_addr;
    // Create a loop of 20 nodes and store it in db. Only even number of them are upgradable.
    let mut ids = stream::iter(0..20)
        .filter_map(|i| {
            let h = host_id;
            let ip = ip_address.clone();
            async move {
                let version = if i % 2 == 0 { "1.0.0" } else { "2.0.0" };
                let blockchain_id = blockchain.id.to_owned();
                let req = create_new_node(
                    i,
                    org.id,
                    &blockchain_id,
                    user.id,
                    version,
                    NodeType::Validator,
                );
                tester
                    .create_node(&req, &h, &ip, &format!("dns-id-{i}"))
                    .await;
                if i % 2 == 0 {
                    Some(req.id.to_string())
                } else {
                    None
                }
            }
        })
        .collect::<Vec<String>>()
        .await;

    let target_version = "2.0.0";

    // Create request object
    let request = api::BabelServiceNotifyRequest {
        uuid: Uuid::new_v4().to_string(),
        config: Some(api::BabelConfig {
            node_version: target_version.to_string(),
            node_type: NodeType::Validator.to_string(),
            protocol: blockchain.name.to_string(),
        }),
    };

    let mut response = tester.send_admin(Service::notify, request).await.unwrap();
    response.node_ids.sort();
    ids.sort();
    assert_eq!(ids, response.node_ids);

    // Check that blockchain supported_node_types was updated with new version
    let mut conn = tester.conn().await;
    assert!(tester
        .blockchain()
        .await
        .properties(&mut conn)
        .await
        .unwrap()
        .into_iter()
        .any(|x| x.version == target_version));
}

#[tokio::test]
async fn test_nothing_to_notify_no_nodes_to_update_all_up_to_date() {
    let tester = &super::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.user().await;
    let org = tester.org_for(&user).await;
    let host_id = tester.host().await.id;
    let ip_address = tester.host().await.ip_addr;
    // Create a loop of 20 nodes and store it in db. Only even number of them are upgradable.
    let _ = stream::iter(0..20)
        .filter_map(|i| {
            let h = host_id;
            let ip = ip_address.clone();
            async move {
                let blockchain_id = blockchain.id.to_owned();
                let req = create_new_node(
                    i,
                    org.id,
                    &blockchain_id,
                    user.id,
                    "2.0.0",
                    NodeType::Validator,
                );
                tester
                    .create_node(&req, &h, &ip, format!("dns-id-{}", i).as_str())
                    .await;
                None
            }
        })
        .collect::<Vec<String>>()
        .await;

    // Create request object
    let request = api::BabelServiceNotifyRequest {
        uuid: Uuid::new_v4().to_string(),
        config: Some(api::BabelConfig {
            node_version: "2.0.0".to_string(),
            node_type: NodeType::Validator.to_string(),
            protocol: blockchain.name.to_string(),
        }),
    };

    let response = tester.send_admin(Service::notify, request).await.unwrap();
    assert!(response.node_ids.is_empty());
}

#[tokio::test]
async fn test_nothing_to_notify_no_nodes_to_update_diff_node_type() {
    let tester = &super::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.user().await;
    let org = tester.org_for(&user).await;
    let host_id = tester.host().await.id;
    let ip_address = tester.host().await.ip_addr;
    // Create a loop of 20 nodes and store it in db. Only even number of them are upgradable.
    let _ = stream::iter(0..20)
        .filter_map(|i| {
            let h = host_id;
            let ip = ip_address.clone();
            async move {
                let blockchain_id = blockchain.id.to_owned();
                let req =
                    create_new_node(i, org.id, &blockchain_id, user.id, "1.0.0", NodeType::Miner);
                tester
                    .create_node(&req, &h, &ip, format!("dns-id-{}", i).as_str())
                    .await;
                None
            }
        })
        .collect::<Vec<String>>()
        .await;

    // Create request object
    let request = api::BabelServiceNotifyRequest {
        uuid: Uuid::new_v4().to_string(),
        config: Some(api::BabelConfig {
            node_version: "2.0.0".to_string(),
            node_type: NodeType::Validator.to_string(),
            protocol: blockchain.name.to_string(),
        }),
    };

    let response = tester.send_admin(Service::notify, request).await.unwrap();
    assert!(response.node_ids.is_empty());
}
