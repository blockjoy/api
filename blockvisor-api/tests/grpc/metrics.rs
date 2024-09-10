use blockvisor_api::grpc::{api, common};
use blockvisor_api::model::node::{Node, NodeHealth, NodeState};
use blockvisor_api::model::Host;
use uuid::Uuid;

use crate::setup::helper::traits::{MetricsService, SocketRpc};
use crate::setup::TestServer;

#[tokio::test]
async fn responds_ok_for_write_node() {
    let test = TestServer::new().await;

    let node_id = test.seed().node.id;
    let metrics = vec![api::NodeMetrics {
        node_id: node_id.to_string(),
        node_status: Some(common::NodeStatus {
            state: common::NodeState::Running as i32,
            next: Some(common::NextState::Stopping as i32),
            protocol: Some(common::ProtocolStatus {
                state: "syncing".into(),
                health: common::NodeHealth::Healthy as i32,
            }),
        }),
        height: Some(10),
        block_age: Some(5),
        consensus: Some(false),
        jobs: vec![common::NodeJob {
            name: "download".to_string(),
            status: 2,
            exit_code: None,
            message: Some("this is going great!".to_string()),
            logs: vec!["[2023-10-03T23:48:21] omg so downloady".to_string()],
            restarts: 2,
            progress: Some(common::NodeJobProgress {
                total: Some(10),
                current: Some(3),
                message: None,
            }),
        }],
    }];

    let jwt = test.org_jwt();
    let req = api::MetricsServiceNodeRequest { metrics };
    test.send_with(MetricsService::node, req, &jwt)
        .await
        .unwrap();

    let mut conn = test.conn().await;
    let node = Node::by_id(node_id, &mut conn).await.unwrap();
    assert_eq!(node.block_height, Some(10));
    assert_eq!(node.block_age, Some(5));
    assert_eq!(node.consensus, Some(false));
    assert_eq!(node.node_state, NodeState::Running);
    // next state writes from client are ignored
    assert_eq!(node.next_state, None);
    assert_eq!(node.protocol_state.as_deref(), Some("syncing"));
    assert_eq!(node.protocol_health, Some(NodeHealth::Healthy));

    let job = node.jobs.unwrap().0.pop().unwrap();
    let progress = job.progress.unwrap();
    assert_eq!(progress.total, Some(10));
    assert_eq!(progress.current, Some(3));
    assert_eq!(progress.message, None);
}

#[tokio::test]
async fn responds_ok_for_write_node_empty() {
    let test = TestServer::new().await;

    let jwt = test.public_host_jwt();
    let req = api::MetricsServiceNodeRequest { metrics: vec![] };
    test.send_with(MetricsService::node, req, &jwt)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_ok_for_write_host() {
    let test = TestServer::new().await;

    let jwt = test.public_host_jwt();
    let host_id = test.seed().host1.id;

    let metrics = api::HostMetrics {
        host_id: host_id.to_string(),
        used_cpu_hundreths: Some(201),
        used_memory_bytes: Some(1123123123123),
        used_disk_bytes: Some(3123213123),
        used_ips: vec!["123.123.123.123".to_string()],
        load_one_percent: Some(1.0),
        load_five_percent: Some(1.0),
        load_fifteen_percent: Some(1.0),
        network_received_bytes: Some(345345345345),
        network_sent_bytes: Some(567567567),
        uptime_seconds: Some(687678678),
    };

    let req = api::MetricsServiceHostRequest {
        metrics: Some(metrics),
    };
    test.send_with(MetricsService::host, req, &jwt)
        .await
        .unwrap();

    let mut conn = test.conn().await;
    let host = Host::by_id(host_id, None, &mut conn).await.unwrap();
    assert_eq!(host.used_cpu_hundreths, Some(201));
    assert_eq!(host.used_memory_bytes, Some(1123123123123));
    assert_eq!(host.used_disk_bytes, Some(3123213123));
    assert_eq!(host.load_one_percent, Some(1.0));
    assert_eq!(host.load_five_percent, Some(1.0));
    assert_eq!(host.load_fifteen_percent, Some(1.0));
    assert_eq!(host.network_received_bytes, Some(345345345345));
    assert_eq!(host.network_sent_bytes, Some(567567567));
    assert_eq!(host.uptime_seconds, Some(687678678));
}

#[tokio::test]
async fn single_failure_doesnt_abort_all_updates() {
    let test = TestServer::new().await;

    let node_id = test.seed().node.id;
    let valid_metric = api::NodeMetrics {
        node_id: node_id.to_string(),
        height: Some(10),
        block_age: Some(5),
        consensus: Some(false),
        node_status: None,
        jobs: vec![common::NodeJob {
            name: "download".to_string(),
            status: 2,
            exit_code: None,
            message: Some("this is going great!".to_string()),
            logs: vec!["[2023-10-03T23:48:21] omg so downloady".to_string()],
            restarts: 2,
            progress: Some(common::NodeJobProgress {
                total: Some(10),
                current: Some(3),
                message: None,
            }),
        }],
    };

    let mut invalid_metric = valid_metric.clone();
    invalid_metric.node_id = Uuid::new_v4().to_string();

    let jwt = test.org_jwt();
    let metrics = vec![valid_metric, invalid_metric];
    let req = api::MetricsServiceNodeRequest { metrics };
    test.send_with(MetricsService::node, req, &jwt)
        .await
        .unwrap_err();

    let mut conn = test.conn().await;
    let node = Node::by_id(node_id, &mut conn).await.unwrap();
    assert_eq!(node.block_height, Some(10));
    assert_eq!(node.block_age, Some(5));
    assert_eq!(node.consensus, Some(false));
    assert_eq!(node.node_state, NodeState::Running);

    let job = node.jobs.unwrap().0.pop().unwrap();
    let progress = job.progress.unwrap();
    assert_eq!(progress.total, Some(10));
    assert_eq!(progress.current, Some(3));
    assert_eq!(progress.message, None);
}
