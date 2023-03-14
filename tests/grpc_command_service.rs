use crate::setup::Tester;
use api::auth::FindableById;
use api::grpc::blockjoy::commands_client::CommandsClient;
use api::grpc::blockjoy::{CommandInfo, PendingCommandsRequest};
use api::models;
use tonic::transport::Channel;
use uuid::Uuid;

mod setup;

type Service = CommandsClient<Channel>;

async fn create_command(
    tester: &Tester,
    node_id: Uuid,
    cmd_type: models::HostCmd,
) -> models::Command {
    let host = tester.host().await;
    let mut conn = tester.conn().await;
    let new_cmd = models::NewCommand {
        host_id: host.id,
        cmd: cmd_type,
        sub_cmd: None,
        node_id: Some(node_id),
    };

    new_cmd.create(&mut conn).await.unwrap()
}

#[tokio::test]
async fn responds_ok_with_single_get() {
    let tester = setup::Tester::new().await;
    let mut conn = tester.conn().await;
    let node = tester.node().await;
    let update = models::UpdateNode {
        id: node.id,
        name: None,
        version: None,
        ip_addr: Some("123.123.123.123"),
        block_height: None,
        node_data: None,
        chain_status: None,
        sync_status: None,
        staking_status: None,
        container_status: None,
        self_update: None,
    };
    update.update(&mut conn).await.unwrap();

    let cmd = create_command(&tester, node.id, models::HostCmd::CreateNode).await;
    let host = models::Host::find_by_id(cmd.host_id, &mut conn)
        .await
        .unwrap();
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let req = CommandInfo {
        id: cmd.id.to_string(),
        response: None,
        exit_code: None,
    };

    tester
        .send_with(Service::get, req, token, refresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_ok_for_update() {
    let tester = setup::Tester::new().await;
    let mut conn = tester.conn().await;
    let node = tester.node().await;
    let cmd = create_command(&tester, node.id, models::HostCmd::CreateNode).await;
    let host = models::Host::find_by_id(cmd.host_id, &mut conn)
        .await
        .unwrap();
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let req = CommandInfo {
        id: cmd.id.to_string(),
        response: Some("hugo boss".to_string()),
        exit_code: Some(98),
    };

    tester
        .send_with(Service::update, req, token, refresh)
        .await
        .unwrap();

    let cmd = models::Command::find_by_id(cmd.id, &mut conn)
        .await
        .unwrap();

    assert_eq!(cmd.response.unwrap(), "hugo boss");
    assert_eq!(cmd.exit_status.unwrap(), 98);
}

#[tokio::test]
async fn responds_ok_for_pending() {
    let tester = setup::Tester::new().await;
    let mut conn = tester.conn().await;
    let node = tester.node().await;
    let update = models::UpdateNode {
        id: node.id,
        name: None,
        version: None,
        ip_addr: Some("123.123.123.123"),
        block_height: None,
        node_data: None,
        chain_status: None,
        sync_status: None,
        staking_status: None,
        container_status: None,
        self_update: None,
    };
    update.update(&mut conn).await.unwrap();
    let cmd = create_command(&tester, node.id, models::HostCmd::CreateNode).await;
    let host = models::Host::find_by_id(cmd.host_id, &mut conn)
        .await
        .unwrap();
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let req = PendingCommandsRequest {
        host_id: host.id.to_string(),
        filter_type: None,
    };

    tester
        .send_with(Service::pending, req, token, refresh)
        .await
        .unwrap();
}
