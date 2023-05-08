use blockvisor_api::grpc::api::{self, BabelConfig, BabelNewVersionRequest};
use blockvisor_api::{models, TestDb};
use futures_util::{stream, StreamExt};

type Service = api::babel_service_client::BabelServiceClient<super::Channel>;

#[tokio::test]
async fn test_notify_success() {
    let tester = super::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.admin_user().await;
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
                let req = models::NewNode {
                    id: uuid::Uuid::new_v4(),
                    org_id: org.id,
                    blockchain_id: blockchain.id,
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
                    name: format!("node-{}", i),
                    version,
                    staking_status: models::NodeStakingStatus::Staked,
                    self_update: true, // important
                    vcpu_count: 0,
                    mem_size_bytes: 0,
                    disk_size_bytes: 0,
                    network: "some network",
                    node_type: models::NodeType::Validator,
                    created_by: user.id,
                    scheduler_similarity: None,
                    scheduler_resource: Some(models::ResourceAffinity::MostResources),
                    allow_ips: serde_json::json!([]),
                    deny_ips: serde_json::json!([]),
                };
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

    // Create request object
    let request = BabelNewVersionRequest {
        uuid: uuid::Uuid::new_v4().to_string(),
        config: Some(BabelConfig {
            node_version: "2.0.0".to_string(),
            node_type: models::NodeType::Validator.to_string(),
            protocol: blockchain.name.to_string(),
        }),
    };

    let mut response = tester.send_admin(Service::notify, request).await.unwrap();
    response.nodes_ids.sort();
    ids.sort();
    assert_eq!(ids, response.nodes_ids);
}
