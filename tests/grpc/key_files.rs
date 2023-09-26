use blockvisor_api::grpc::api;
use blockvisor_api::models::node::key_file::NewNodeKeyFile;
use tonic::transport::Channel;

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type Service = api::key_file_service_client::KeyFileServiceClient<Channel>;

#[tokio::test]
async fn responds_not_found_with_invalid_node_id() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let req = api::KeyFileServiceListRequest {
        node_id: uuid::Uuid::new_v4().to_string(),
    };
    let status = test.send_with(Service::list, req, &jwt).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound, "{status:?}");
}

#[tokio::test]
async fn responds_ok_with_valid_node_id() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let node_id = test.seed().node.id;
    let file = NewNodeKeyFile {
        name: "my-key.txt",
        content:
            "asödlfasdf asdfjaöskdjfalsdjföasjdf afa sdffasdfasldfjasödfj asdföalksdföalskdjfa",
        node_id,
    };

    let mut conn = test.conn().await;
    NewNodeKeyFile::bulk_create(vec![file], &mut conn)
        .await
        .unwrap();

    let req = api::KeyFileServiceListRequest {
        node_id: node_id.to_string(),
    };
    test.send_with(Service::list, req, &jwt).await.unwrap();
}

#[tokio::test]
async fn responds_not_found_with_invalid_node_id_for_save() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let key_file = api::Keyfile {
        name: "new keyfile".to_string(),
        content: "üöäß@niesfiefasd".to_string().into_bytes(),
    };
    let req = api::KeyFileServiceCreateRequest {
        node_id: uuid::Uuid::new_v4().to_string(),
        key_files: vec![key_file],
    };
    let status = test
        .send_with(Service::create, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_ok_with_valid_node_id_for_save() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let node_id = test.seed().node.id;
    let key_file = api::Keyfile {
        name: "new keyfile".to_string(),
        content: "üöäß@niesfiefasd".to_string().into_bytes(),
    };
    let req = api::KeyFileServiceCreateRequest {
        node_id: node_id.to_string(),
        key_files: vec![key_file],
    };
    test.send_with(Service::create, req, &jwt).await.unwrap();
}

#[tokio::test]
async fn responds_error_with_same_node_id_name_twice_for_save() {
    let test = TestServer::new().await;

    let jwt = test.host_jwt();
    let node_id = test.seed().node.id;
    let key_file = api::Keyfile {
        name: "new keyfile".to_string(),
        content: "üöäß@niesfiefasd".to_string().into_bytes(),
    };
    let req = api::KeyFileServiceCreateRequest {
        node_id: node_id.to_string(),
        key_files: vec![key_file.clone()],
    };
    test.send_with(Service::create, req, &jwt).await.unwrap();

    let req = api::KeyFileServiceCreateRequest {
        node_id: node_id.to_string(),
        key_files: vec![key_file],
    };

    let status = test
        .send_with(Service::create, req, &jwt)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::AlreadyExists)
}
