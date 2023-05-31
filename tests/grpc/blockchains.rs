use blockvisor_api::grpc::api;

type Service = api::blockchain_service_client::BlockchainServiceClient<super::Channel>;

#[tokio::test]
async fn responds_ok_for_get_existing() {
    let tester = super::Tester::new().await;
    let req = api::BlockchainServiceGetRequest {
        id: "ab5d8cfc-77b1-4265-9fee-ba71ba9de092".to_string(),
    };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_not_found_for_get_nonexisting() {
    let tester = super::Tester::new().await;
    let req = api::BlockchainServiceGetRequest {
        id: "6a9efd38-0c5a-4ab0-bda2-5f308f850565".to_string(),
    };
    let status = tester.send_admin(Service::get, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_not_found_for_get_deleted() {
    let tester = super::Tester::new().await;
    let req = api::BlockchainServiceGetRequest {
        id: "13f25489-bf9b-4667-9f18-f8caa32fa4a9".to_string(),
    };
    let status = tester.send_admin(Service::get, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn can_list_blockchains() {
    let tester = super::Tester::new().await;
    let req = api::BlockchainServiceListRequest {};
    tester.send_admin(Service::list, req).await.unwrap();
}
