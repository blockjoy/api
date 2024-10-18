use blockvisor_api::auth::resource::NodeId;
use blockvisor_api::grpc::api;
use blockvisor_api::model::command::{Command, CommandType, ExitCode, NewCommand};
use blockvisor_api::model::host::Host;
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
async fn responds_ok_for_update() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    let node_id = test.seed().node.id;
    let cmd = create_command(&test, node_id, CommandType::NodeCreate).await;
    let host = Host::by_id(cmd.host_id, &mut conn).await.unwrap();

    let claims = test.host_claims_for(host.id);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let req = api::CommandServiceUpdateRequest {
        id: cmd.id.to_string(),
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
    let node = Node::by_id(node_id, &mut conn).await.unwrap();
    let update = UpdateNode {
        display_name: Some("pending"),
        ..Default::default()
    };
    node.update(&update, &mut conn).await.unwrap();

    let cmd = create_command(&test, node_id, CommandType::NodeCreate).await;
    let host = Host::by_id(cmd.host_id, &mut conn).await.unwrap();
    let claims = test.host_claims_for(host.id);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let req = api::CommandServicePendingRequest {
        host_id: host.id.to_string(),
        filter_type: None,
    };
    test.send_with(CommandService::pending, req, &jwt)
        .await
        .unwrap();
}
