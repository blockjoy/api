mod setup;

use api::grpc::blockjoy::{self, metrics_service_client};
use api::models;
use tonic::transport;

type Service = metrics_service_client::MetricsServiceClient<transport::Channel>;

#[tokio::test]
async fn responds_ok_for_write_node() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let node = tester.node().await;
    let mut metrics = std::collections::HashMap::new();
    let metric = blockjoy::NodeMetrics {
        height: Some(10),
        block_age: Some(5),
        staking_status: Some(4),
        consensus: Some(false),
    };
    metrics.insert(node.id.to_string(), metric);
    let req = blockjoy::NodeMetricsRequest { metrics };
    tester
        .send_with(Service::node, req, auth, refresh)
        .await
        .unwrap();
    let node = tester.node().await;
    assert_eq!(node.block_height, Some(10));
    assert_eq!(node.block_age, Some(5));
    assert_eq!(node.staking_status, models::NodeStakingStatus::Validating);
    assert_eq!(node.consensus, Some(false));
}

#[tokio::test]
async fn responds_ok_for_write_node_empty() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let metrics = std::collections::HashMap::new();
    let req = blockjoy::NodeMetricsRequest { metrics };
    tester
        .send_with(Service::node, req, auth, refresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_ok_for_write_host() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let mut metrics = std::collections::HashMap::new();
    let metric = blockjoy::HostMetrics {
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
    let req = blockjoy::HostMetricsRequest { metrics };
    tester
        .send_with(Service::host, req, auth, refresh)
        .await
        .unwrap();
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
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let metrics = std::collections::HashMap::new();
    let req = blockjoy::HostMetricsRequest { metrics };
    tester
        .send_with(Service::host, req, auth, refresh)
        .await
        .unwrap();
}
