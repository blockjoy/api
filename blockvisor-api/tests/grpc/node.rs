use blockvisor_api::database::seed::{
    ARCHIVE_ID_1, ARCHIVE_ID_2, DISK_BYTES, IMAGE_ID, MEMORY_BYTES, MORE_RESOURCES_KEY, ORG_ID,
};
use blockvisor_api::grpc::{api, common};
use blockvisor_api::model::command::Command;
use blockvisor_api::model::schema::commands;
use blockvisor_api::model::Node;
use blockvisor_api::util::sql::{Tag, Tags};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use tonic::Code;
use uuid::Uuid;

use crate::setup::helper::traits::{NodeService, SocketRpc};
use crate::setup::TestServer;

#[tokio::test]
async fn create_a_new_node() {
    let test = TestServer::new().await;

    let create_req = |org_id, image_id, new_values, add_rules| api::NodeServiceCreateRequest {
        org_id,
        image_id,
        old_node_id: None,
        placement: Some(scheduler_placement()),
        new_values,
        add_rules,
        tags: None,
    };

    // an org admin can't create a node with an invalid org_id
    let req = create_req(Uuid::new_v4().into(), IMAGE_ID.into(), vec![], vec![]);
    let result = test.send_admin(NodeService::create, req).await;
    assert_eq!(result.unwrap_err().code(), Code::PermissionDenied);

    // an org admin can create a new node
    let req = create_req(ORG_ID.into(), IMAGE_ID.into(), vec![], vec![]);
    let result = test.send_admin(NodeService::create, req.clone()).await;
    let mut result = result.unwrap();
    let node = result.nodes.pop().unwrap();
    assert_eq!(node.image_id, IMAGE_ID);
    assert_eq!(node.host_id, test.seed().host1.id.to_string());

    let config = node.config.unwrap();
    let image = config.image.unwrap();
    assert_eq!(image.archive_id, ARCHIVE_ID_1);
    let vm = config.vm.unwrap();
    assert_eq!(vm.cpu_cores, 1);
    assert_eq!(vm.memory_bytes, MEMORY_BYTES as u64);
    assert_eq!(vm.disk_bytes, DISK_BYTES as u64);

    // creating a node with MORE_RESOURCES_KEY uses a different archive
    let new_values = vec![property(MORE_RESOURCES_KEY, "moar")];
    let req = create_req(ORG_ID.into(), IMAGE_ID.into(), new_values, vec![]);
    let result = test.send_admin(NodeService::create, req.clone()).await;
    let mut result = result.unwrap();
    let node = result.nodes.pop().unwrap();
    assert_eq!(node.image_id, IMAGE_ID);
    assert_eq!(node.host_id, test.seed().host1.id.to_string());

    // confirm that a node with MORE_RESOURCES_KEY uses additional resources
    let config = node.config.unwrap();
    let image = config.image.unwrap();
    assert_eq!(image.archive_id, ARCHIVE_ID_2);
    let vm = config.vm.unwrap();
    assert_eq!(vm.cpu_cores, 2);
    assert_eq!(vm.memory_bytes, 2 * MEMORY_BYTES as u64);
    assert_eq!(vm.disk_bytes, 2 * DISK_BYTES as u64);

    // can choose a specific host to create a node on
    let mut host2_req = create_req(ORG_ID.into(), IMAGE_ID.into(), vec![], vec![]);
    host2_req.placement = Some(host_placement(test.seed().host2.id));
    let result = test
        .send_admin(NodeService::create, host2_req.clone())
        .await;
    let mut result = result.unwrap();
    let node = result.nodes.pop().unwrap();
    assert_eq!(node.image_id, IMAGE_ID);
    assert_eq!(node.host_id, test.seed().host2.id.to_string());

    // unless that host has now ran out of resources
    let result = test.send_admin(NodeService::create, host2_req).await;
    assert_eq!(result.unwrap_err().code(), Code::FailedPrecondition);
}

fn scheduler_placement() -> common::NodePlacement {
    common::NodePlacement {
        placement: Some(common::node_placement::Placement::Scheduler(
            common::NodeScheduler {
                resource: common::ResourceAffinity::MostResources.into(),
                similarity: None,
                region: None,
            },
        )),
    }
}

fn host_placement<S: ToString>(host_id: S) -> common::NodePlacement {
    common::NodePlacement {
        placement: Some(common::node_placement::Placement::HostId(
            host_id.to_string(),
        )),
    }
}

fn property<S: Into<String>>(key: S, value: S) -> common::ImagePropertyValue {
    common::ImagePropertyValue {
        key: key.into(),
        value: value.into(),
    }
}

#[tokio::test]
async fn update_a_node_config() {
    let test = TestServer::new().await;
    let node_id = test.seed().node.id;

    let update_req = |node_id| api::NodeServiceUpdateConfigRequest {
        node_id,
        auto_upgrade: Some(true),
        new_org_id: None,
        new_display_name: Some("<script>alert('XSS');</script>".to_string()),
        new_note: Some("milk, eggs, bread and copious snacks".to_string()),
        new_values: vec![],
        new_firewall: None,
        update_tags: Some(common::UpdateTags {
            update: Some(common::update_tags::Update::OverwriteTags(common::Tags {
                tags: vec![common::Tag {
                    name: "updated-node".to_string(),
                }],
            })),
        }),
    };

    // fails for unknown id
    let req = update_req(Uuid::new_v4().to_string());
    let status = test
        .send_admin(NodeService::update_config, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::NotFound);

    // ok for an org admin
    let req = update_req(node_id.to_string());
    test.send_admin(NodeService::update_config, req)
        .await
        .unwrap();

    let mut conn = test.conn().await;
    let node = Node::by_id(node_id, &mut conn).await.unwrap();

    assert!(node.auto_upgrade);
    assert_eq!(node.note.unwrap(), "milk, eggs, bread and copious snacks");
    assert_eq!(node.display_name.unwrap(), "<script>alert('XSS');</script>");
    assert_eq!(
        node.tags,
        Tags(vec![Tag::new("updated-node".to_string()).unwrap()])
    );

    validate_commands(&test).await;
}

#[tokio::test]
async fn get_an_existing_node() {
    let test = TestServer::new().await;

    let get_req = |node_id| api::NodeServiceGetRequest { node_id };

    // fails for unknown id
    let req = get_req(Uuid::new_v4().to_string());
    let status = test.send_admin(NodeService::get, req).await.unwrap_err();
    assert_eq!(status.code(), Code::NotFound);

    // ok for an org member
    let req = get_req(test.seed().node.id.to_string());
    test.send_admin(NodeService::get, req).await.unwrap();

    validate_commands(&test).await;
}

#[tokio::test]
async fn start_and_stop_a_node() {
    let test = TestServer::new().await;
    let node_id = test.seed().node.id;

    let req = api::NodeServiceStartRequest {
        node_id: node_id.to_string(),
    };
    test.send_admin(NodeService::start, req).await.unwrap();

    let req = api::NodeServiceStopRequest {
        node_id: node_id.to_string(),
    };
    test.send_admin(NodeService::stop, req).await.unwrap();

    let req = api::NodeServiceRestartRequest {
        node_id: node_id.to_string(),
    };
    test.send_admin(NodeService::restart, req).await.unwrap();

    validate_commands(&test).await;
}

#[tokio::test]
async fn report_a_node_status() {
    let test = TestServer::new().await;
    let node = &test.seed().node;

    let report_req = || api::NodeServiceReportStatusRequest {
        node_id: node.id.to_string(),
        config_id: node.config_id.to_string(),
        status: Some(common::NodeStatus {
            state: common::NodeState::Stopped as i32,
            next: None,
            protocol: None,
        }),
        p2p_address: None,
    };

    // fails for an org admin
    let req = report_req();
    let status = test
        .send_admin(NodeService::report_status, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);

    // ok for org token
    let jwt = test.org_jwt();
    let req = report_req();
    test.send_with(NodeService::report_status, req, &jwt)
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_an_existing_node() {
    let test = TestServer::new().await;

    let delete_req = || api::NodeServiceDeleteRequest {
        node_id: test.seed().node.id.to_string(),
    };

    // fails without perm
    let req = delete_req();
    let status = test
        .send_member(NodeService::delete, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::PermissionDenied);

    // ok with perm
    let req = delete_req();
    test.send_admin(NodeService::delete, req).await.unwrap();

    validate_commands(&test).await;
}

async fn validate_commands(test: &TestServer) {
    let mut conn = test.conn().await;
    let commands: Vec<Command> = commands::table
        .filter(commands::node_id.is_null())
        .get_results(&mut conn)
        .await
        .unwrap();

    assert!(commands.is_empty());
}
