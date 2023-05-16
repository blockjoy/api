use blockvisor_api::grpc::api;
use blockvisor_api::{models, TestDb};
use futures_util::{stream, StreamExt};

type Service = api::babel_service_client::BabelServiceClient<super::Channel>;

fn create_new_node<'a>(
    index: usize,
    org_id: &'a uuid::Uuid,
    blockchain_id: &'a uuid::Uuid,
    user_id: &'a uuid::Uuid,
    version: &'a str,
    node_type: &'a models::NodeType,
) -> models::NewNode<'a> {
    let id = uuid::Uuid::new_v4();
    let name = format!("node-{}-{}", index, id);
    models::NewNode {
        id,
        org_id: org_id.to_owned(),
        blockchain_id: blockchain_id.to_owned(),
        properties: serde_json::to_value(models::NodeProperties {
            version: None,
            properties: Some(vec![]),
        })
        .unwrap(),
        chain_status: models::NodeChainStatus::Unknown,
        sync_status: models::NodeSyncStatus::Syncing,
        container_status: models::ContainerStatus::Installing,
        block_height: None,
        node_data: None,
        name,
        version,
        staking_status: models::NodeStakingStatus::Staked,
        self_update: true,
        vcpu_count: 0,
        mem_size_bytes: 0,
        disk_size_bytes: 0,
        network: "some network",
        node_type: *node_type,
        created_by: user_id.to_owned(),
        scheduler_similarity: None,
        scheduler_resource: Some(models::ResourceAffinity::MostResources),
        allow_ips: serde_json::json!([]),
        deny_ips: serde_json::json!([]),
    }
}

#[tokio::test]
async fn test_notify_success() {
    let tester = super::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.user().await;
    let org = tester.org_for(&user).await;
    let host_id = tester.host().await.id;
    let ip_address = tester.host().await.ip_addr;
    // Create a loop of 20 nodes and store it in db. Only even number of them are upgradable.
    let mut ids = stream::iter(0..20)
        .filter_map(|i| {
            let t = tester.pool.clone();
            let h = host_id;
            let ip = ip_address.clone();
            async move {
                let version = if i % 2 == 0 { "1.0.0" } else { "2.0.0" };
                let org_id = org.id.to_owned();
                let blockchain_id = blockchain.id.to_owned();
                let user_id = user.id.to_owned();
                let req = create_new_node(
                    i,
                    &org_id,
                    &blockchain_id,
                    &user_id,
                    version,
                    &models::NodeType::Validator,
                );
                let mut conn = t.conn().await.unwrap();
                TestDb::create_node(&req, &h, &ip, format!("dns-id-{}", i).as_str(), &mut conn)
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
        uuid: uuid::Uuid::new_v4().to_string(),
        config: Some(api::BabelConfig {
            node_version: target_version.to_string(),
            node_type: models::NodeType::Validator.to_string(),
            protocol: blockchain.name.to_string(),
        }),
    };

    let mut response = tester.send_admin(Service::notify, request).await.unwrap();
    response.node_ids.sort();
    ids.sort();
    assert_eq!(ids, response.node_ids);

    // Check that blockchain supported_node_types was updated with new version
    assert!(tester
        .blockchain()
        .await
        .supported_node_types()
        .unwrap()
        .into_iter()
        .any(|x| x.version == target_version));
}

#[tokio::test]
async fn test_nothing_to_notify_no_nodes_to_update_all_up_to_date() {
    let tester = super::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.user().await;
    let org = tester.org_for(&user).await;
    let host_id = tester.host().await.id;
    let ip_address = tester.host().await.ip_addr;
    // Create a loop of 20 nodes and store it in db. Only even number of them are upgradable.
    let _ = stream::iter(0..20)
        .filter_map(|i| {
            let t = tester.pool.clone();
            let h = host_id;
            let ip = ip_address.clone();
            async move {
                let org_id = org.id.to_owned();
                let blockchain_id = blockchain.id.to_owned();
                let user_id = user.id.to_owned();
                let req = create_new_node(
                    i,
                    &org_id,
                    &blockchain_id,
                    &user_id,
                    "2.0.0",
                    &models::NodeType::Validator,
                );
                let mut conn = t.conn().await.unwrap();
                TestDb::create_node(&req, &h, &ip, format!("dns-id-{}", i).as_str(), &mut conn)
                    .await;
                None
            }
        })
        .collect::<Vec<String>>()
        .await;

    // Create request object
    let request = api::BabelServiceNotifyRequest {
        uuid: uuid::Uuid::new_v4().to_string(),
        config: Some(api::BabelConfig {
            node_version: "2.0.0".to_string(),
            node_type: models::NodeType::Validator.to_string(),
            protocol: blockchain.name.to_string(),
        }),
    };

    let response = tester.send_admin(Service::notify, request).await.unwrap();
    assert!(response.node_ids.is_empty());
}

#[tokio::test]
async fn test_nothing_to_notify_no_nodes_to_update_diff_node_type() {
    let tester = super::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.user().await;
    let org = tester.org_for(&user).await;
    let host_id = tester.host().await.id;
    let ip_address = tester.host().await.ip_addr;
    // Create a loop of 20 nodes and store it in db. Only even number of them are upgradable.
    let _ = stream::iter(0..20)
        .filter_map(|i| {
            let t = tester.pool.clone();
            let h = host_id;
            let ip = ip_address.clone();
            async move {
                let org_id = org.id.to_owned();
                let blockchain_id = blockchain.id.to_owned();
                let user_id = user.id.to_owned();
                let req = create_new_node(
                    i,
                    &org_id,
                    &blockchain_id,
                    &user_id,
                    "1.0.0",
                    &models::NodeType::Miner,
                );
                let mut conn = t.conn().await.unwrap();
                TestDb::create_node(&req, &h, &ip, format!("dns-id-{}", i).as_str(), &mut conn)
                    .await;
                None
            }
        })
        .collect::<Vec<String>>()
        .await;

    // Create request object
    let request = api::BabelServiceNotifyRequest {
        uuid: uuid::Uuid::new_v4().to_string(),
        config: Some(api::BabelConfig {
            node_version: "2.0.0".to_string(),
            node_type: models::NodeType::Validator.to_string(),
            protocol: blockchain.name.to_string(),
        }),
    };

    let response = tester.send_admin(Service::notify, request).await.unwrap();
    assert!(response.node_ids.is_empty());
}
