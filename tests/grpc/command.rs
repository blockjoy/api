use blockvisor_api::grpc::api;

type Service = api::commands_client::CommandServiceClient<transport::Channel>;

/// Returns a semtantically invalid command. This can be used to assert for status codes to your
/// liking.
async fn valid_command() -> (super::Tester, api::CommandRequest) {
    let tester = super::Tester::new().await;
    let req = api::CommandRequest {
         
        id: tester.host().await.id.to_string(),
        params: vec![],
    };
    (tester, req)
}

/// Returns a semtantically invalid command. This can be used to assert for InvalidArgument
/// responses.
async fn invalid_command() -> (super::Tester, api::CommandRequest) {
    let tester = super::Tester::new().await;
    let req = api::CommandRequest {
         
        id: "".to_string(),
        params: vec![],
    };
    (tester, req)
}

/// TODO
#[tokio::test]
#[ignore]
async fn responds_ok_for_create_node() {
    let (tester, req) = valid_command().await;
    tester.send_admin(Service::create_node, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_for_create_node() {
    let (tester, req) = valid_command().await;
    let status = tester
        .send_admin(Service::create_node, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn responds_invalid_argument_for_create_node() {
    let (tester, req) = invalid_command().await;
    let status = tester
        .send_admin(Service::create_node, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

/// TODO
#[tokio::test]
#[ignore]
async fn responds_ok_for_delete_node() {
    let (tester, req) = valid_command().await;
    tester.send_admin(Service::delete_node, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_for_delete_node() {
    let (tester, req) = valid_command().await;
    let status = tester
        .send_admin(Service::delete_node, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn responds_invalid_argument_for_delete_node() {
    let (tester, req) = invalid_command().await;
    let status = tester
        .send_admin(Service::delete_node, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

/// TODO
#[tokio::test]
#[ignore]
async fn responds_ok_for_start_node() {
    let (tester, req) = valid_command().await;
    tester.send_admin(Service::start_node, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_for_start_node() {
    let (tester, req) = valid_command().await;
    let status = tester
        .send_admin(Service::start_node, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn responds_invalid_argument_for_start_node() {
    let (tester, req) = invalid_command().await;
    let status = tester
        .send_admin(Service::start_node, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

/// TODO
#[tokio::test]
#[ignore]
async fn responds_ok_for_stop_node() {
    let (tester, req) = valid_command().await;
    tester.send_admin(Service::stop_node, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_for_stop_node() {
    let (tester, req) = valid_command().await;
    let status = tester
        .send_admin(Service::stop_node, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn responds_invalid_argument_for_stop_node() {
    let (tester, req) = invalid_command().await;
    let status = tester
        .send_admin(Service::stop_node, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

/// TODO
#[tokio::test]
#[ignore]
async fn responds_ok_for_restart_node() {
    let (tester, req) = valid_command().await;
    tester.send_admin(Service::restart_node, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_for_restart_node() {
    let (tester, req) = valid_command().await;
    let status = tester
        .send_admin(Service::restart_node, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn responds_invalid_argument_for_restart_node() {
    let (tester, req) = invalid_command().await;
    let status = tester
        .send_admin(Service::restart_node, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

/// TODO
#[tokio::test]
#[ignore]
async fn responds_ok_for_create_host() {
    let (tester, req) = valid_command().await;
    tester.send_admin(Service::create_host, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_for_create_host() {
    let (tester, req) = valid_command().await;
    let status = tester
        .send_admin(Service::create_host, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn responds_invalid_argument_for_create_host() {
    let (tester, req) = invalid_command().await;
    let status = tester
        .send_admin(Service::create_host, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

/// TODO
#[tokio::test]
#[ignore]
async fn responds_ok_for_delete_host() {
    let (tester, req) = valid_command().await;
    tester.send_admin(Service::delete_host, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_for_delete_host() {
    let (tester, req) = valid_command().await;
    let status = tester
        .send_admin(Service::delete_host, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn responds_invalid_argument_for_delete_host() {
    let (tester, req) = invalid_command().await;
    let status = tester
        .send_admin(Service::delete_host, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

/// TODO
#[tokio::test]
#[ignore]
async fn responds_ok_for_start_host() {
    let (tester, req) = valid_command().await;
    tester.send_admin(Service::start_host, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_for_start_host() {
    let (tester, req) = valid_command().await;
    let status = tester
        .send_admin(Service::start_host, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn responds_invalid_argument_for_start_host() {
    let (tester, req) = invalid_command().await;
    let status = tester
        .send_admin(Service::start_host, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

/// TODO
#[tokio::test]
#[ignore]
async fn responds_ok_for_stop_host() {
    let (tester, req) = valid_command().await;
    tester.send_admin(Service::stop_host, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_for_stop_host() {
    let (tester, req) = valid_command().await;
    let status = tester
        .send_admin(Service::stop_host, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn responds_invalid_argument_for_stop_host() {
    let (tester, req) = invalid_command().await;
    let status = tester
        .send_admin(Service::stop_host, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

/// TODO
#[tokio::test]
#[ignore]
async fn responds_ok_for_restart_host() {
    let (tester, req) = valid_command().await;
    tester.send_admin(Service::restart_host, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_for_restart_host() {
    let (tester, req) = valid_command().await;
    let status = tester
        .send_admin(Service::restart_host, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

#[tokio::test]
async fn responds_invalid_argument_for_restart_host() {
    let (tester, req) = invalid_command().await;
    let status = tester
        .send_admin(Service::restart_host, req)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}
