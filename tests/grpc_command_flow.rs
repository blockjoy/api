mod setup;

use api::grpc::blockjoy::{self, command_flow_client, info_update};
use api::grpc::blockjoy_ui::{self, command_service_client::CommandServiceClient};
use setup::TestStream;
use std::marker::PhantomData;
use tonic::transport;

type Service = command_flow_client::CommandFlowClient<transport::Channel>;

#[tokio::test]
async fn command_flow_works() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let req = blockjoy::InfoUpdate {
        info: Some(info_update::Info::Node(blockjoy::NodeInfo {
            id: tester.node().await.id.to_string(),
            name: None,
            ip: None,
            block_height: None,
            onchain_name: None,
            app_status: Some(blockjoy::node_info::ApplicationStatus::Electing as i32),
            container_status: None,
            sync_status: Some(1),
            staking_status: Some(1),
            self_update: Some(false),
            address: Some("0xasdfasdf".to_string()),
        })),
    };

    let mut stream = tester
        .open_stream_with(Service::commands, tokio_stream::once(req), token, refresh)
        .await
        .unwrap();
    stream.assert_empty().await;
}

#[tokio::test]
async fn non_existent_node() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let req = blockjoy::InfoUpdate {
        info: Some(info_update::Info::Node(blockjoy::NodeInfo {
            id: "1f950d23-9e53-4ff6-aff7-4ddf6ee39f55".to_string(),
            name: None,
            ip: None,
            block_height: None,
            onchain_name: None,
            app_status: Some(8),
            container_status: None,
            sync_status: Some(1),
            staking_status: Some(1),
            self_update: Some(true),
            address: Some("0xasdfasdf".to_string()),
        })),
    };

    let mut stream = tester
        .open_stream_with(Service::commands, tokio_stream::once(req), token, refresh)
        .await
        .unwrap();
    let msg = stream.assert_receives().await.unwrap_err();
    assert_eq!(msg.message(), "Record not found.");
}

/// This test makes sure that when we send an update to a node, the command_flow streams for that
/// node receive a message, but the command_flow streams for another node does not receive a
/// message. That is, we filter the messages correctly, and you only get updates about the node you
/// are listening to.
#[tokio::test]
async fn concurrent_streams() {
    type CmdService = CommandServiceClient<transport::Channel>;

    let tester = setup::Tester::new().await;

    let host1 = tester.host().await;
    let token1 = tester.host_token(&host1);
    let refresh1 = tester.refresh_for(&token1);
    let (tkn1, rfr1) = (token1.clone(), refresh1.clone());
    let mut stream1 = tester
        .open_stream_with(Service::commands, Eternal::new(), tkn1, rfr1)
        .await
        .unwrap();

    let host2 = tester.host2().await;
    let token2 = tester.host_token(&host2);
    let refresh2 = tester.refresh_for(&token2);
    let mut stream2 = tester
        .open_stream_with(Service::commands, Eternal::new(), token2, refresh2)
        .await
        .unwrap();

    let req = blockjoy_ui::CommandRequest {
        meta: Some(tester.meta()),
        id: host1.id.to_string(),
        params: vec![blockjoy_ui::Parameter {
            name: "resource_id".to_string(),
            value: tester.node().await.id.to_string(),
        }],
    };
    tester
        .send_with(CmdService::start_node, req, token1, refresh1)
        .await
        .unwrap();

    let resp = stream1.assert_receives().await.unwrap();
    // assert the the message sent to host1 somewhat makes sense
    assert!(resp.r#type.is_some());
    // assert that no message is sent to host2, because we only updated host1
    stream2.assert_empty().await;
}

/// The eternal stream that never closes. We use this to keep the stream open after we insert data,
/// so the server doesn't think that the client has stopped listening and we can check for a
/// response.
struct Eternal<T>(PhantomData<T>);

impl<T> Eternal<T> {
    fn new() -> Self {
        Self(Default::default())
    }
}

impl<T> futures_util::Stream for Eternal<T> {
    type Item = T;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Pending
    }
}
