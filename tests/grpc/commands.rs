use blockvisor_api::auth::resource::NodeId;
use blockvisor_api::grpc::api;
use blockvisor_api::models;

type Service = api::command_service_client::CommandServiceClient<super::Channel>;

async fn create_command(
    tester: &super::Tester,
    node_id: NodeId,
    cmd_type: models::CommandType,
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
async fn responds_ok_for_update() {
    let tester = super::Tester::new().await;
    let mut conn = tester.conn().await;
    let node = tester.node().await;
    let cmd = create_command(&tester, node.id, models::CommandType::CreateNode).await;
    let host = models::Host::find_by_id(cmd.host_id, &mut conn)
        .await
        .unwrap();

    let claims = tester.host_token(&host);
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let req = api::CommandServiceUpdateRequest {
        id: cmd.id.to_string(),
        response: Some("hugo boss".to_string()),
        exit_code: Some(98),
    };

    tester.send_with(Service::update, req, &jwt).await.unwrap();

    let cmd = models::Command::find_by_id(cmd.id, &mut conn)
        .await
        .unwrap();

    assert_eq!(cmd.response.unwrap(), "hugo boss");
    assert_eq!(cmd.exit_status.unwrap(), 98);
}

#[tokio::test]
async fn responds_ok_for_pending() {
    let tester = super::Tester::new().await;
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
        address: None,
        allow_ips: None,
        deny_ips: None,
    };
    update.update(&mut conn).await.unwrap();
    let cmd = create_command(&tester, node.id, models::CommandType::CreateNode).await;
    let host = models::Host::find_by_id(cmd.host_id, &mut conn)
        .await
        .unwrap();

    let claims = tester.host_token(&host);
    let jwt = tester.cipher().jwt.encode(&claims).unwrap();

    let req = api::CommandServicePendingRequest {
        host_id: host.id.to_string(),
        filter_type: None,
    };

    tester.send_with(Service::pending, req, &jwt).await.unwrap();
}
