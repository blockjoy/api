use blockvisor_api::grpc::{api, common};
use blockvisor_api::models::command::{Command, CommandType};
use blockvisor_api::models::node::Node;
use blockvisor_api::models::schema;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use tonic::transport::Channel;
use uuid::Uuid;

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type Service = api::node_service_client::NodeServiceClient<Channel>;

#[tokio::test]
async fn can_create_multiple() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    let req = api::NodeServiceCreateRequest {
        org_id: test.seed().org.id.to_string(),
        blockchain_id: test.seed().blockchain.id.to_string(),
        node_type: common::NodeType::Validator.into(),
        properties: vec![],
        version: "3.3.0".to_string(),
        network: "some network".to_string(),
        placement: Some(api::NodePlacement {
            placement: Some(api::node_placement::Placement::Multiple(
                api::MultipleNodes {
                    node_counts: vec![api::NodeCount {
                        host_id: test.seed().host.id.to_string(),
                        node_count: 2,
                    }],
                },
            )),
        }),
        allow_ips: vec![],
        deny_ips: vec![],
    };

    let resp = test.send_admin(Service::create, req).await.unwrap();
    assert_eq!(resp.nodes.len(), 2);

    let id1 = resp.nodes[0].id.parse().unwrap();
    let id2 = resp.nodes[1].id.parse().unwrap();

    let node1 = Node::by_id(id1, &mut conn).await.unwrap();
    assert!(!node1.dns_record_id.is_empty());

    let node2 = Node::by_id(id2, &mut conn).await.unwrap();
    assert!(!node2.dns_record_id.is_empty());
}

#[tokio::test]
async fn responds_ok_for_update_config() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let node_id = test.seed().node.id;
    let req = api::NodeServiceUpdateConfigRequest {
        id: node_id.to_string(),
        self_update: Some(true),
        allow_ips: vec![api::FilteredIpAddr {
            ip: "127.0.0.1".to_string(),
            description: Some("wow so allowed".to_string()),
        }],
        deny_ips: vec![api::FilteredIpAddr {
            ip: "127.0.0.2".to_string(),
            description: Some("wow so denied".to_string()),
        }],
        org_id: None,
        note: Some("milk, eggs, bread and copious snacks".to_string()),
        display_name: Some("<script>alert('XSS');</script>".to_string()),
    };

    test.send_with(Service::update_config, req, &jwt)
        .await
        .unwrap();

    let mut conn = test.conn().await;
    let node = Node::by_id(node_id, &mut conn).await.unwrap();

    // Some assertions that the update actually worked
    assert!(node.self_update);

    let allowed = node.allow_ips().unwrap()[0].clone();
    assert_eq!(allowed.ip, "127.0.0.1");
    assert_eq!(allowed.description.unwrap(), "wow so allowed");

    let denied = node.deny_ips().unwrap()[0].clone();
    assert_eq!(denied.ip, "127.0.0.2");
    assert_eq!(denied.description.unwrap(), "wow so denied");

    assert_eq!(
        node.note,
        Some("milk, eggs, bread and copious snacks".to_string())
    );

    assert_eq!(
        node.display_name,
        "<script>alert('XSS');</script>".to_string()
    );

    validate_command(&test).await;
}

#[tokio::test]
async fn responds_not_found_without_any_for_get() {
    let test = TestServer::new().await;
    let req = api::NodeServiceGetRequest {
        id: Uuid::new_v4().to_string(),
    };
    let status = test.send_admin(Service::get, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
    validate_command(&test).await;
}

#[tokio::test]
async fn responds_ok_with_id_for_get() {
    let test = TestServer::new().await;
    let node = &test.seed().node;
    let req = api::NodeServiceGetRequest {
        id: node.id.to_string(),
    };
    test.send_admin(Service::get, req).await.unwrap();
    validate_command(&test).await;
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_create() {
    let test = TestServer::new().await;
    let req = api::NodeServiceCreateRequest {
        org_id: test.seed().org.id.to_string(),
        blockchain_id: test.seed().blockchain.id.to_string(),
        node_type: common::NodeType::Validator.into(),
        properties: vec![],
        version: "3.3.0".to_string(),
        network: "some network".to_string(),
        placement: Some(api::NodePlacement {
            placement: Some(api::node_placement::Placement::HostId(
                test.seed().host.id.to_string(),
            )),
        }),
        allow_ips: vec![api::FilteredIpAddr {
            ip: "127.0.0.1".to_string(),
            description: Some("wow so allowed".to_string()),
        }],
        deny_ips: vec![api::FilteredIpAddr {
            ip: "127.0.0.2".to_string(),
            description: Some("wow so denied".to_string()),
        }],
    };
    let resp = test.send_admin(Service::create, req).await.unwrap();

    // assert that it really exists
    let req = api::NodeServiceGetRequest {
        id: resp.nodes[0].id.clone(),
    };
    let resp = test.send_admin(Service::get, req).await.unwrap();
    let node = resp.node.unwrap();

    let allowed = node.allow_ips[0].clone();
    assert_eq!(allowed.ip, "127.0.0.1");
    assert_eq!(allowed.description.unwrap(), "wow so allowed");

    let denied = node.deny_ips[0].clone();
    assert_eq!(denied.ip, "127.0.0.2");
    assert_eq!(denied.description.unwrap(), "wow so denied");
    validate_command(&test).await;
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_create_schedule() {
    let test = TestServer::new().await;
    let req = api::NodeServiceCreateRequest {
        org_id: test.seed().org.id.to_string(),
        blockchain_id: test.seed().blockchain.id.to_string(),
        node_type: common::NodeType::Validator.into(),
        properties: vec![],
        version: "3.3.0".to_string(),
        network: "some network".to_string(),
        placement: Some(api::NodePlacement {
            placement: Some(api::node_placement::Placement::Scheduler(
                api::NodeScheduler {
                    similarity: None,
                    resource: api::node_scheduler::ResourceAffinity::MostResources.into(),
                    region: "moneyland".to_string(),
                },
            )),
        }),
        allow_ips: vec![],
        deny_ips: vec![],
    };
    test.send_root(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_invalid_argument_with_invalid_data_for_create() {
    let test = TestServer::new().await;
    let req = api::NodeServiceCreateRequest {
        // This is an invalid uuid so the api call should fail.
        org_id: "wowowowowow".to_string(),
        blockchain_id: test.seed().blockchain.id.to_string(),
        node_type: common::NodeType::Validator.into(),
        properties: vec![],
        version: "3.3.0".to_string(),
        network: "some network".to_string(),
        placement: Some(api::NodePlacement {
            placement: Some(api::node_placement::Placement::Scheduler(
                api::NodeScheduler {
                    similarity: None,
                    resource: api::node_scheduler::ResourceAffinity::MostResources.into(),
                    region: "moneyland".to_string(),
                },
            )),
        }),
        allow_ips: vec![],
        deny_ips: vec![],
    };
    let status = test.send_root(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument, "{status:?}");
    validate_command(&test).await;
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_update_config() {
    let test = TestServer::new().await;
    let req = api::NodeServiceUpdateConfigRequest {
        id: test.seed().node.id.to_string(),
        self_update: Some(false),
        allow_ips: vec![],
        deny_ips: vec![],
        org_id: None,
        note: None,
        display_name: None,
    };
    test.send_admin(Service::update_config, req).await.unwrap();
    validate_command(&test).await;
}

#[tokio::test]
async fn responds_ok_for_start_stop_restart() {
    let test = TestServer::new().await;
    let node_id = test.seed().node.id;

    let req = api::NodeServiceStartRequest {
        id: node_id.to_string(),
    };
    test.send_admin(Service::start, req).await.unwrap();
    validate_command(&test).await;

    let req = api::NodeServiceStopRequest {
        id: node_id.to_string(),
    };
    test.send_admin(Service::stop, req).await.unwrap();
    validate_command(&test).await;

    let req = api::NodeServiceRestartRequest {
        id: node_id.to_string(),
    };
    test.send_admin(Service::restart, req).await.unwrap();
    validate_command(&test).await;
}

#[tokio::test]
async fn responds_permission_denied_with_member_token_for_update_status() {
    let test = TestServer::new().await;

    let jwt = test.member_jwt().await;
    let req = api::NodeServiceUpdateStatusRequest {
        id: test.seed().node.id.to_string(),
        version: Some("v2".to_string()),
        container_status: None,
        address: Some("address".to_string()),
    };
    let status = test
        .send_with(Service::update_status, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_internal_with_invalid_data_for_update_config() {
    let test = TestServer::new().await;
    let req = api::NodeServiceUpdateConfigRequest {
        // This is an invalid uuid so the api call should fail.
        id: "wowowow".to_string(),
        ..Default::default()
    };
    let status = test
        .send_admin(Service::update_config, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
    validate_command(&test).await;
}

#[tokio::test]
async fn responds_not_found_with_invalid_id_for_update_config() {
    let test = TestServer::new().await;
    let req = api::NodeServiceUpdateConfigRequest {
        // This uuid will not exist, so the api call should fail.
        id: Uuid::new_v4().to_string(),
        ..Default::default()
    };
    let status = test
        .send_admin(Service::update_config, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound, "{status:?}");
    validate_command(&test).await;
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_delete() {
    let test = TestServer::new().await;
    let req = api::NodeServiceDeleteRequest {
        id: test.seed().node.id.to_string(),
    };
    test.send_admin(Service::delete, req).await.unwrap();
    validate_command(&test).await;
}

async fn validate_command(test: &TestServer) {
    let mut conn = test.conn().await;
    let commands_empty: Vec<Command> = schema::commands::table
        .filter(
            schema::commands::node_id
                .is_null()
                .and(schema::commands::command_type.ne(CommandType::NodeDelete))
                .or(schema::commands::command_type
                    .eq(CommandType::NodeDelete)
                    .and(schema::commands::node_id.is_null())),
        )
        .load::<Command>(&mut conn)
        .await
        .unwrap();

    assert_eq!(commands_empty.len(), 0);
}
