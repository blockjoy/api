use blockvisor_api::grpc::api;
use blockvisor_api::models;

type Service = api::key_file_service_client::KeyFileServiceClient<super::Channel>;

#[tokio::test]
async fn responds_ok_with_invalid_node_id() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let req = api::KeyFileServiceListRequest {
        node_id: uuid::Uuid::new_v4().to_string(),
    };
    tester
        .send_with(Service::list, req, auth, refresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_ok_with_valid_node_id() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let mut conn = tester.conn().await;
    let node = tester.node().await;
    let new_node_key_file = models::NewNodeKeyFile {
        name: "my-key.txt",
        content:
            "asödlfasdf asdfjaöskdjfalsdjföasjdf afa sdffasdfasldfjasödfj asdföalksdföalskdjfa",
        node_id: node.id,
    };
    new_node_key_file.create(&mut conn).await.unwrap();
    let req = api::KeyFileServiceListRequest {
        node_id: node.id.to_string(),
    };
    tester
        .send_with(Service::list, req, auth, refresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_not_found_with_invalid_node_id_for_save() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let key_file = api::Keyfile {
        name: "new keyfile".to_string(),
        content: "üöäß@niesfiefasd".to_string().into_bytes(),
    };
    let req = api::KeyFileServiceCreateRequest {
        node_id: uuid::Uuid::new_v4().to_string(),
        key_files: vec![key_file],
    };
    let status = tester
        .send_with(Service::create, req, auth, refresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_ok_with_valid_node_id_for_save() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let node = tester.node().await;
    let key_file = api::Keyfile {
        name: "new keyfile".to_string(),
        content: "üöäß@niesfiefasd".to_string().into_bytes(),
    };
    let req = api::KeyFileServiceCreateRequest {
        node_id: node.id.to_string(),
        key_files: vec![key_file],
    };
    tester
        .send_with(Service::create, req, auth, refresh)
        .await
        .unwrap();
}

#[tokio::test]
async fn responds_error_with_same_node_id_name_twice_for_save() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let auth = tester.host_token(&host);
    let refresh = tester.refresh_for(&auth);
    let node = tester.node().await;
    let key_file = api::Keyfile {
        name: "new keyfile".to_string(),
        content: "üöäß@niesfiefasd".to_string().into_bytes(),
    };
    let req = api::KeyFileServiceCreateRequest {
        node_id: node.id.to_string(),
        key_files: vec![key_file.clone()],
    };
    let (auth_, refresh_) = (auth.clone(), refresh.clone());
    tester
        .send_with(Service::create, req, auth_, refresh_)
        .await
        .unwrap();

    let req = api::KeyFileServiceCreateRequest {
        node_id: node.id.to_string(),
        key_files: vec![key_file],
    };
    let status = tester
        .send_with(Service::create, req, auth, refresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument)
}
