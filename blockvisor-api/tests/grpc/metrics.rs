use std::collections::HashMap;

use blockvisor_api::grpc::api;
use blockvisor_api::models::node::{Node, NodeStatus, StakingStatus, SyncStatus};
use blockvisor_api::models::Host;
use tonic::transport::Channel;

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type Service = api::metrics_service_client::MetricsServiceClient<Channel>;

#[tokio::test]
async fn responds_ok_for_write_node() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let node_id = test.seed().node.id;

    let mut metrics = HashMap::new();
    let metric = api::NodeMetrics {
        height: Some(10),
        block_age: Some(5),
        staking_status: Some(4),
        consensus: Some(false),
        application_status: Some(8),
        sync_status: Some(2),

        jobs: vec![api::NodeJob {
            name: "download".to_string(),
            status: 2,
            exit_code: None,
            message: Some("this is going great!".to_string()),
            logs: vec!["[2023-10-03T23:48:21] omg so downloady".to_string()],
            restarts: 2,
            progress: Some(api::NodeJobProgress {
                total: Some(10),
                current: Some(3),
                message: None,
            }),
        }],
    };
    metrics.insert(node_id.to_string(), metric);
    let req = api::MetricsServiceNodeRequest { metrics };
    test.send_with(Service::node, req, &jwt).await.unwrap();

    let mut conn = test.conn().await;
    let node = Node::by_id(node_id, &mut conn).await.unwrap();
    assert_eq!(node.block_height, Some(10));
    assert_eq!(node.block_age, Some(5));
    assert_eq!(node.staking_status, Some(StakingStatus::Validating));
    assert_eq!(node.consensus, Some(false));
    assert_eq!(node.node_status, NodeStatus::Electing);
    assert_eq!(node.sync_status, SyncStatus::Synced);
    let job = node.jobs().unwrap().pop().unwrap();
    let progress = job.progress.unwrap();
    assert_eq!(progress.total, Some(10));
    assert_eq!(progress.current, Some(3));
    assert_eq!(progress.message, None);
}

#[tokio::test]
async fn responds_ok_for_write_node_empty() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let metrics = HashMap::new();
    let req = api::MetricsServiceNodeRequest { metrics };
    test.send_with(Service::node, req, &jwt).await.unwrap();
}

#[tokio::test]
async fn responds_ok_for_write_host() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let host_id = test.seed().host.id;

    let mut metrics = HashMap::new();
    let metric = api::HostMetrics {
        used_cpu: Some(201),
        used_memory: Some(1123123123123),
        used_disk_space: Some(3123213123),
        used_ips: vec!["123.123.123.123".to_string()],
        load_one: Some(1.0),
        load_five: Some(1.0),
        load_fifteen: Some(1.0),
        network_received: Some(345345345345),
        network_sent: Some(567567567),
        uptime: Some(687678678),
    };
    metrics.insert(host_id.to_string(), metric);
    let req = api::MetricsServiceHostRequest { metrics };
    test.send_with(Service::host, req, &jwt).await.unwrap();

    let mut conn = test.conn().await;
    let host = Host::by_id(host_id, &mut conn).await.unwrap();
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
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let metrics = HashMap::new();
    let req = api::MetricsServiceHostRequest { metrics };
    test.send_with(Service::host, req, &jwt).await.unwrap();
}

#[tokio::test]
async fn single_failure_doesnt_abort_all_updates() {
    let test = TestServer::new().await;
    let jwt = test.host_jwt();

    let mut metrics = std::collections::HashMap::new();
    let metric = api::NodeMetrics {
        height: Some(10),
        block_age: Some(5),
        staking_status: Some(4),
        consensus: Some(false),
        application_status: Some(8),
        sync_status: Some(2),
        jobs: vec![api::NodeJob {
            name: "download".to_string(),
            status: 2,
            exit_code: None,
            message: Some("this is going great!".to_string()),
            logs: vec!["[2023-10-03T23:48:21] omg so downloady".to_string()],
            restarts: 2,
            progress: Some(api::NodeJobProgress {
                total: Some(10),
                current: Some(3),
                message: None,
            }),
        }],
    };
    let node_id = test.seed().node.id;
    metrics.insert(node_id.to_string(), metric.clone());
    metrics.insert(uuid::Uuid::from_u128(0).to_string(), metric);
    let req = api::MetricsServiceNodeRequest { metrics };
    test.send_with(Service::node, req, &jwt).await.unwrap_err();

    let mut conn = test.conn().await;
    let node = Node::by_id(node_id, &mut conn).await.unwrap();
    assert_eq!(node.block_height, Some(10));
    assert_eq!(node.block_age, Some(5));
    assert_eq!(node.staking_status, Some(StakingStatus::Validating));
    assert_eq!(node.consensus, Some(false));
    assert_eq!(node.node_status, NodeStatus::Electing);
    assert_eq!(node.sync_status, SyncStatus::Synced);
    let job = node.jobs().unwrap().pop().unwrap();
    let progress = job.progress.unwrap();
    assert_eq!(progress.total, Some(10));
    assert_eq!(progress.current, Some(3));
    assert_eq!(progress.message, None);
}
