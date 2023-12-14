use blockvisor_api::auth::resource::NodeId;
use blockvisor_api::grpc::api;
use blockvisor_api::models::command::{Command, CommandType, ExitCode, NewCommand};
use blockvisor_api::models::host::Host;
use blockvisor_api::models::node::UpdateNode;
use tonic::transport::Channel;

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type Service = api::command_service_client::CommandServiceClient<Channel>;

async fn create_command(test: &TestServer, node_id: NodeId, cmd_type: CommandType) -> Command {
    let mut conn = test.conn().await;
    let new_cmd = NewCommand {
        host_id: test.seed().host.id,
        cmd: cmd_type,
        node_id: Some(node_id),
    };

    new_cmd.create(&mut conn).await.unwrap()
}

#[tokio::test]
async fn responds_ok_for_update() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    let node_id = test.seed().node.id;
    let cmd = create_command(&test, node_id, CommandType::CreateNode).await;
    let host = Host::find_by_id(cmd.host_id, &mut conn).await.unwrap();

    let claims = test.host_claims_for(host.id);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let req = api::CommandServiceUpdateRequest {
        id: cmd.id.to_string(),
        exit_message: Some("hugo boss".to_string()),
        exit_code: Some(api::CommandExitCode::ServiceBroken.into()),
        retry_hint_seconds: Some(10),
    };

    test.send_with(Service::update, req, &jwt).await.unwrap();

    let cmd = Command::find_by_id(cmd.id, &mut conn).await.unwrap();

    assert_eq!(cmd.exit_message.unwrap(), "hugo boss");
    assert_eq!(cmd.exit_code.unwrap(), ExitCode::ServiceBroken);
    assert_eq!(cmd.retry_hint_seconds.unwrap(), 10);
}

#[tokio::test]
async fn responds_ok_for_pending() {
    let test = TestServer::new().await;
    let mut conn = test.conn().await;

    let node_id = test.seed().node.id;
    let update = UpdateNode {
        id: node_id,
        name: None,
        version: None,
        ip_addr: Some("123.123.123.123"),
        block_height: None,
        node_data: None,
        node_status: None,
        sync_status: None,
        staking_status: None,
        container_status: None,
        self_update: None,
        address: None,
        allow_ips: None,
        deny_ips: None,
    };
    update.update(&mut conn).await.unwrap();

    let cmd = create_command(&test, node_id, CommandType::CreateNode).await;
    let host = Host::find_by_id(cmd.host_id, &mut conn).await.unwrap();

    let claims = test.host_claims_for(host.id);
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    let req = api::CommandServicePendingRequest {
        host_id: host.id.to_string(),
        filter_type: None,
    };
    test.send_with(Service::pending, req, &jwt).await.unwrap();
}
