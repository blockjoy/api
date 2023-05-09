use blockvisor_api::grpc::api;
use blockvisor_api::models;

type Service = api::metrics_service_client::MetricsServiceClient<super::Channel>;

#[tokio::test]
async fn responds_ok_for_write_node() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let jwt = tester.host_token(&host);
    let node = tester.node().await;
    let mut metrics = std::collections::HashMap::new();
    let metric = api::NodeMetrics {
        height: Some(10),
        block_age: Some(5),
        staking_status: Some(4),
        consensus: Some(false),
        application_status: Some(8),
        sync_status: Some(2),
    };
    metrics.insert(node.id.to_string(), metric);
    let req = api::MetricsServiceNodeRequest { metrics };
    tester.send_with(Service::node, req, jwt).await.unwrap();
    let node = tester.node().await;
    assert_eq!(node.block_height, Some(10));
    assert_eq!(node.block_age, Some(5));
    assert_eq!(
        node.staking_status,
        Some(models::NodeStakingStatus::Validating)
    );
    assert_eq!(node.consensus, Some(false));
    assert_eq!(node.chain_status, models::NodeChainStatus::Electing);
    assert_eq!(node.sync_status, models::NodeSyncStatus::Synced);
}

#[tokio::test]
async fn responds_ok_for_write_node_empty() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let jwt = tester.host_token(&host);
    let metrics = std::collections::HashMap::new();
    let req = api::MetricsServiceNodeRequest { metrics };
    tester.send_with(Service::node, req, jwt).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_write_host() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let jwt = tester.host_token(&host);

    let mut metrics = std::collections::HashMap::new();
    let metric = api::HostMetrics {
        used_cpu: Some(201),
        used_memory: Some(1123123123123),
        used_disk_space: Some(3123213123),
        load_one: Some(1.0),
        load_five: Some(1.0),
        load_fifteen: Some(1.0),
        network_received: Some(345345345345),
        network_sent: Some(567567567),
        uptime: Some(687678678),
    };
    metrics.insert(host.id.to_string(), metric);
    let req = api::MetricsServiceHostRequest { metrics };
    tester.send_with(Service::host, req, jwt).await.unwrap();
    let host = tester.host().await;
    assert_eq!(host.used_cpu, Some(201));
    assert_eq!(host.used_memory, Some(1123123123123));
    assert_eq!(host.used_disk_space, Some(3123213123));
    assert_eq!(host.load_one, Some(1.0));
    assert_eq!(host.load_five, Some(1.0));
    assert_eq!(host.load_fifteen, Some(1.0));
    assert_eq!(host.network_received, Some(345345345345));
    assert_eq!(host.network_sent, Some(567567567));
    assert_eq!(host.uptime, Some(687678678));
}

#[tokio::test]
async fn responds_ok_for_write_host_empty() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let jwt = tester.host_token(&host);

    let metrics = std::collections::HashMap::new();
    let req = api::MetricsServiceHostRequest { metrics };
    tester.send_with(Service::host, req, jwt).await.unwrap();
}
