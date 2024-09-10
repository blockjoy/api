use blockvisor_api::auth::claims::Granted;
use blockvisor_api::auth::resource::NodeId;
use blockvisor_api::auth::AuthZ;
use blockvisor_api::grpc::api;
use blockvisor_api::model::command::{Command, CommandType, ExitCode, NewCommand};
use blockvisor_api::model::node::UpdateNode;
use blockvisor_api::model::Node;

use crate::setup::helper::traits::{CommandService, SocketRpc};
use crate::setup::TestServer;

async fn create_command(test: &TestServer, node_id: NodeId, cmd_type: CommandType) -> Command {
    let mut conn = test.conn().await;
    let node = Node::by_id(node_id, &mut conn).await.unwrap();
    let new_cmd = NewCommand::node(&node, cmd_type).unwrap();
    new_cmd.create(&mut conn).await.unwrap()
}

#[tokio::test]
async fn node_create_failed() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    let node_id = test.seed().node.id;
    let cmd = create_command(&test, node_id, CommandType::NodeCreate).await;

    let jwt = test.org_jwt();
    let req = api::CommandServiceUpdateRequest {
        command_id: cmd.id.to_string(),
        exit_message: Some("hugo boss".to_string()),
        exit_code: Some(api::CommandExitCode::ServiceBroken.into()),
        retry_hint_seconds: Some(10),
    };

    test.send_with(CommandService::update, req, &jwt)
        .await
        .unwrap();

    let cmd = Command::by_id(cmd.id, &mut conn).await.unwrap();

    assert_eq!(cmd.exit_message.unwrap(), "hugo boss");
    assert_eq!(cmd.exit_code.unwrap(), ExitCode::ServiceBroken);
    assert_eq!(cmd.retry_hint_seconds.unwrap(), 10);
}

#[tokio::test]
async fn responds_ok_for_pending() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    let node_id = test.seed().node.id;
    let host_id = test.seed().host1.id;

    let authz = AuthZ {
        claims: test.member_claims().await,
        granted: Granted::default(),
    };

    let update = UpdateNode {
        org_id: None,
        host_id: None,
        display_name: None,
        auto_upgrade: None,
        ip_address: Some("123.123.123.123".parse().unwrap()),
        ip_gateway: None,
        note: None,
        tags: None,
    };
    update.apply(node_id, &authz, &mut conn).await.unwrap();
    create_command(&test, node_id, CommandType::NodeCreate).await;

    let jwt = test.org_jwt();
    let req = api::CommandServicePendingRequest {
        host_id: host_id.to_string(),
        filter_type: None,
    };
    test.send_with(CommandService::pending, req, &jwt)
        .await
        .unwrap();
}
