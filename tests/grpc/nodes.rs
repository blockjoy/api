use std::ops::DerefMut;

use blockvisor_api::grpc::api;
use blockvisor_api::models::command::{Command, CommandType};
use blockvisor_api::models::node::Node;
use blockvisor_api::models::{schema, User};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use uuid::Uuid;

use crate::setup::helper::rpc;
use crate::setup::Tester;

type Service = api::node_service_client::NodeServiceClient<super::Channel>;

#[tokio::test]
async fn responds_ok_for_update_config() {
    let tester = Tester::new().await;

    let host = tester.host().await;
    let claims = tester.host_token(&host);
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let node = tester.node().await;
    let node_id = node.id.to_string();
    let req = api::NodeServiceUpdateConfigRequest {
        id: node_id.clone(),
        self_update: Some(true),
        allow_ips: vec![api::FilteredIpAddr {
            ip: "127.0.0.1".to_string(),
            description: Some("wow so allowed".to_string()),
        }],
        deny_ips: vec![api::FilteredIpAddr {
            ip: "127.0.0.2".to_string(),
            description: Some("wow so denied".to_string()),
        }],
    };

    tester
        .send_with(Service::update_config, req, &jwt)
        .await
        .unwrap();

    let mut conn = tester.conn().await;
    let node = Node::find_by_id(node_id.parse().unwrap(), &mut conn)
        .await
        .unwrap();

    // Some assertions that the update actually worked
    assert!(node.self_update);

    let allowed = node.allow_ips().unwrap()[0].clone();
    assert_eq!(allowed.ip, "127.0.0.1");
    assert_eq!(allowed.description.unwrap(), "wow so allowed");

    let denied = node.deny_ips().unwrap()[0].clone();
    assert_eq!(denied.ip, "127.0.0.2");
    assert_eq!(denied.description.unwrap(), "wow so denied");
    validate_command(&tester).await;
}

#[tokio::test]
async fn responds_not_found_without_any_for_get() {
    let tester = Tester::new().await;
    let req = api::NodeServiceGetRequest {
        id: Uuid::new_v4().to_string(),
    };
    let status = tester.send_admin(Service::get, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
    validate_command(&tester).await;
}

#[tokio::test]
async fn responds_ok_with_id_for_get() {
    let tester = Tester::new().await;
    let node = tester.node().await;
    let req = api::NodeServiceGetRequest {
        id: node.id.to_string(),
    };
    tester.send_admin(Service::get, req).await.unwrap();
    validate_command(&tester).await;
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_create() {
    let tester = Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.user().await;
    let org = tester.org_for(&user).await;
    let req = api::NodeServiceCreateRequest {
        org_id: org.id.to_string(),
        blockchain_id: blockchain.id.to_string(),
        node_type: api::NodeType::Validator.into(),
        properties: vec![],
        version: "3.3.0".to_string(),
        network: "some network".to_string(),
        placement: Some(api::NodePlacement {
            // This was changed it because otherwise it would make a real call to Cookbook which is
            // not desirable and it would fail because it's not running.
            placement: Some(api::node_placement::Placement::HostId(
                tester.host().await.id.to_string(),
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
    let node = tester.send_admin(Service::create, req).await.unwrap();

    // assert that it really exists
    let req = api::NodeServiceGetRequest {
        id: node.node.unwrap().id,
    };
    let resp = tester.send_admin(Service::get, req).await.unwrap();
    let node = resp.node.unwrap();

    let allowed = node.allow_ips[0].clone();
    assert_eq!(allowed.ip, "127.0.0.1");
    assert_eq!(allowed.description.unwrap(), "wow so allowed");

    let denied = node.deny_ips[0].clone();
    assert_eq!(denied.ip, "127.0.0.2");
    assert_eq!(denied.description.unwrap(), "wow so denied");
    validate_command(&tester).await;
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_create_schedule() {
    let tester = Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.user().await;
    let org = tester.org_for(&user).await;
    let req = api::NodeServiceCreateRequest {
        org_id: org.id.to_string(),
        blockchain_id: blockchain.id.to_string(),
        node_type: api::NodeType::Validator.into(),
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
    tester.send_admin(Service::create, req).await.unwrap();
}

#[tokio::test]
async fn responds_invalid_argument_with_invalid_data_for_create() {
    let tester = Tester::new().await;
    let blockchain = tester.blockchain().await;
    let req = api::NodeServiceCreateRequest {
        // This is an invalid uuid so the api call should fail.
        org_id: "wowowowowow".to_string(),
        blockchain_id: blockchain.id.to_string(),
        node_type: api::NodeType::Api.into(),
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
    let status = tester.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
    validate_command(&tester).await;
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_update_config() {
    let tester = Tester::new().await;
    let node = tester.node().await;
    let req = api::NodeServiceUpdateConfigRequest {
        id: node.id.to_string(),
        self_update: Some(false),
        allow_ips: vec![],
        deny_ips: vec![],
    };
    tester
        .send_admin(Service::update_config, req)
        .await
        .unwrap();
    validate_command(&tester).await;
}

#[tokio::test]
async fn responds_ok_for_start_stop_restart() {
    let tester = Tester::new().await;
    let node = tester.node().await;
    let req = api::NodeServiceStartRequest {
        id: node.id.to_string(),
    };
    tester.send_admin(Service::start, req).await.unwrap();
    validate_command(&tester).await;
    let req = api::NodeServiceStopRequest {
        id: node.id.to_string(),
    };
    tester.send_admin(Service::stop, req).await.unwrap();
    validate_command(&tester).await;
    let req = api::NodeServiceRestartRequest {
        id: node.id.to_string(),
    };
    tester.send_admin(Service::restart, req).await.unwrap();
    validate_command(&tester).await;
}

#[tokio::test]
async fn responds_permission_denied_with_user_token_for_update_status() {
    let tester = Tester::new().await;

    let user = tester.user().await;
    let claims = tester.user_token(&user).await;
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let node = tester.node().await;
    let req = api::NodeServiceUpdateStatusRequest {
        id: node.id.to_string(),
        version: Some("v2".to_string()),
        container_status: None,
        address: Some("address".to_string()),
    };
    let status = tester
        .send_with(Service::update_status, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn responds_internal_with_invalid_data_for_update_config() {
    let tester = Tester::new().await;
    let req = api::NodeServiceUpdateConfigRequest {
        // This is an invalid uuid so the api call should fail.
        id: "wowowow".to_string(),
        ..Default::default()
    };
    let status = tester
        .send_admin(Service::update_config, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
    validate_command(&tester).await;
}

#[tokio::test]
async fn responds_not_found_with_invalid_id_for_update_config() {
    let tester = Tester::new().await;
    let req = api::NodeServiceUpdateConfigRequest {
        // This uuid will not exist, so the api call should fail.
        id: Uuid::new_v4().to_string(),
        ..Default::default()
    };
    let status = tester
        .send_admin(Service::update_config, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound, "{status:?}");
    validate_command(&tester).await;
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_delete() {
    let tester = Tester::new().await;
    let node = tester.node().await;
    let req = api::NodeServiceDeleteRequest {
        id: node.id.to_string(),
    };
    tester.send_admin(Service::delete, req).await.unwrap();
    validate_command(&tester).await;
}

#[tokio::test]
async fn can_get_node_if_blockjoy_admin() {
    let mut test = Tester::new().await;

    let org_user = rpc::new_org_user(&mut test).await;

    let node = test.node().await;
    let req = api::NodeServiceGetRequest {
        id: node.id.to_string(),
    };

    // different org can't get node
    let result = test
        .send_with(Service::get, req.clone(), &org_user.jwt)
        .await;
    assert!(result.is_err());

    let conn = &mut test.conn().await;
    let mut user = User::find_by_id(org_user.user_id, conn).await.unwrap();
    user.is_blockjoy_admin = true;
    User::update(&user, conn).await.unwrap();

    // unless they are also a blockjoy admin
    let result = test.send_with(Service::get, req, &org_user.jwt).await;
    assert!(result.is_ok());
}

async fn validate_command(tester: &Tester) {
    let mut conn = tester.conn().await;
    let commands_empty: Vec<Command> = schema::commands::table
        .filter(
            schema::commands::node_id
                .is_null()
                .and(schema::commands::cmd.ne(CommandType::DeleteNode))
                .or(schema::commands::cmd
                    .eq(CommandType::DeleteNode)
                    .and(schema::commands::sub_cmd.is_null())
                    .and(schema::commands::node_id.is_null())),
        )
        .load::<Command>(conn.deref_mut())
        .await
        .unwrap();

    assert_eq!(commands_empty.len(), 0);
}
