use blockvisor_api::auth::resource::{OrgId, UserId};
use blockvisor_api::grpc::api;
use blockvisor_api::models::node::{
    ContainerStatus, NewNode, NodeChainStatus, NodeStakingStatus, NodeSyncStatus, NodeType,
    ResourceAffinity,
};
use blockvisor_api::models::BlockchainId;
use futures_util::{stream, StreamExt};
use tonic::transport::Channel;
use uuid::Uuid;

use crate::setup::TestServer;

type Service = api::babel_service_client::BabelServiceClient<Channel>;

fn create_new_node(test: &Test, index: usize, version: &str, node_type: NodeType) -> NewNode<'_> {
    let id = Uuid::new_v4().into();
    let name = format!("node-{index}-{id}");

    NewNode {
        id,
        org_id: test.seed().org.id,
        blockchain_id: test.seed().blockchain.id,
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
        created_by: test.seed().user.id,
        scheduler_similarity: None,
        scheduler_resource: Some(ResourceAffinity::MostResources),
        scheduler_region: None,
        allow_ips: serde_json::json!([]),
        deny_ips: serde_json::json!([]),
    }
}

async fn create_node(
    test: &Test,
    node: &NewNode<'_>,
    host_id: HostId,
    ip_addr: &str,
    dns_id: &str,
) {
    let mut conn = self.conn().await;
    diesel::insert_into(nodes::table)
        .values((
            node,
            nodes::host_id.eq(host_id),
            nodes::ip_addr.eq(ip_addr),
            nodes::dns_record_id.eq(dns_id),
        ))
        .execute(&mut conn)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_notify_success() {
    let test = TestServer::new().await;
    let host_id = test.host().await.id;
    let ip_address = test.host().await.ip_addr;
    // Create a loop of 20 nodes and store it in db. Only even number of them are upgradable.
    let mut ids = stream::iter(0..20)
        .filter_map(|i| {
            let ip = ip_address.clone();
            async move {
                let version = if i % 2 == 0 { "1.0.0" } else { "2.0.0" };
                let req = create_new_node(&test, i, version, NodeType::Validator);
                create_node(&test, &req, host_id, &ip, &format!("dns-id-{i}")).await;
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

    let mut response = test.send_admin(Service::notify, request).await.unwrap();
    response.node_ids.sort();
    ids.sort();
    assert_eq!(ids, response.node_ids);

    // Check that blockchain supported_node_types was updated with new version
    let mut conn = test.conn().await;
    assert!(test
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
    let test = TestServer::new().await;
    let host_id = test.host().await.id;
    let ip_address = test.host().await.ip_addr;
    // Create a loop of 20 nodes and store it in db. Only even number of them are upgradable.
    let _ = stream::iter(0..20)
        .filter_map(|i| {
            let ip = ip_address.clone();
            async move {
                let req = create_new_node(&test, i, "2.0.0", NodeType::Validator);
                create_node(&test, &req, host_id, &ip, format!("dns-id-{}", i).as_str()).await;
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

    let response = test.send_admin(Service::notify, request).await.unwrap();
    assert!(response.node_ids.is_empty());
}

#[tokio::test]
async fn test_nothing_to_notify_no_nodes_to_update_diff_node_type() {
    let test = TestServer::new().await;
    let host_id = test.host().await.id;
    let ip_address = test.host().await.ip_addr;
    // Create a loop of 20 nodes and store it in db. Only even number of them are upgradable.
    let _ = stream::iter(0..20)
        .filter_map(|i| {
            let ip = ip_address.clone();
            async move {
                let req = create_new_node(i, "1.0.0", NodeType::Miner);
                create_node(&test, &req, host_id, &ip, format!("dns-id-{}", i).as_str()).await;
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

    let response = test.send_admin(Service::notify, request).await.unwrap();
    assert!(response.node_ids.is_empty());
}
