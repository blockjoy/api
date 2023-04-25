use blockvisor_api::auth::FindableById;
use blockvisor_api::grpc::api;
use blockvisor_api::models;

type Service = api::nodes_client::NodesClient<super::Channel>;

#[tokio::test]
async fn responds_ok_for_update() {
    let tester = super::Tester::new().await;
    let host = tester.host().await;
    let token = tester.host_token(&host);
    let refresh = tester.refresh_for(&token);
    let node = tester.node().await;
    let node_id = node.id.to_string();
    let req = api::UpdateNodeRequest {
        id: node_id.clone(),
        self_update: Some(true),
        container_status: None,
        address: None,
        version: Some("newer is always better".to_string()),
        allow_ips: vec![api::FilteredIpAddr {
            ip: "127.0.0.1".to_string(),
            description: Some("wow so allowed".to_string()),
        }],
        deny_ips: vec![api::FilteredIpAddr {
            ip: "127.0.0.2".to_string(),
            description: Some("wow so denied".to_string()),
        }],
    };

    tester
        .send_with(Service::update, req, token, refresh)
        .await
        .unwrap();

    let mut conn = tester.conn().await;
    let node = models::Node::find_by_id(node_id.parse().unwrap(), &mut conn)
        .await
        .unwrap();

    // Some assertions that the update actually worked
    assert!(node.self_update);

    let allowed = node.allow_ips().unwrap()[0].clone();
    assert_eq!(allowed.ip, "127.0.0.1");
    assert_eq!(allowed.description.unwrap(), "wow so allowed");

    let denied = node.deny_ips().unwrap()[0].clone();
    assert_eq!(denied.ip, "127.0.0.2");
    assert_eq!(denied.description.unwrap(), "wow so denied");
}

#[tokio::test]
async fn responds_not_found_without_any_for_get() {
    let tester = super::Tester::new().await;
    let req = api::GetNodeRequest {
        id: uuid::Uuid::new_v4().to_string(),
    };
    let status = tester.send_admin(Service::get, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn responds_ok_with_id_for_get() {
    let tester = super::Tester::new().await;
    let node = tester.node().await;
    let req = api::GetNodeRequest {
        id: node.id.to_string(),
    };
    tester.send_admin(Service::get, req).await.unwrap();
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_create() {
    let tester = super::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let user = tester.admin_user().await;
    let org = tester.org_for(&user).await;
    let req = api::CreateNodeRequest {
        org_id: org.id.to_string(),
        blockchain_id: blockchain.id.to_string(),
        node_type: api::node::NodeType::Validator.into(),
        properties: vec![],
        version: "3.3.0".to_string(),
        network: "some network".to_string(),
        scheduler: Some(api::NodeScheduler {
            similarity: None,
            resource: api::node_scheduler::ResourceAffinity::MostResources.into(),
        }),
        allow_ips: vec![api::FilteredIpAddr {
            ip: "127.0.0.1".to_string(),
            description: Some("wow so allowed".to_string()),
        }],
        deny_ips: vec![api::FilteredIpAddr {
            ip: "127.0.0.2".to_string(),
            description: Some("wow so denied".to_string()),
        }],
    };
    let node = tester.send_admin(Service::create, req).await.unwrap();

    // assert that it really exists
    let req = api::GetNodeRequest {
        id: node.node.unwrap().id,
    };
    let resp = tester.send_admin(Service::get, req).await.unwrap();
    let node = resp.node.unwrap();

    let allowed = node.allow_ips[0].clone();
    assert_eq!(allowed.ip, "127.0.0.1");
    assert_eq!(allowed.description.unwrap(), "wow so allowed");

    let denied = node.deny_ips[0].clone();
    assert_eq!(denied.ip, "127.0.0.2");
    assert_eq!(denied.description.unwrap(), "wow so denied");
}

#[tokio::test]
async fn responds_invalid_argument_with_invalid_data_for_create() {
    let tester = super::Tester::new().await;
    let blockchain = tester.blockchain().await;
    let req = api::CreateNodeRequest {
        // This is an invalid uuid so the api call should fail.
        org_id: "wowowowowow".to_string(),
        blockchain_id: blockchain.id.to_string(),
        node_type: api::node::NodeType::Api.into(),
        properties: vec![],
        version: "3.3.0".to_string(),
        network: "some network".to_string(),
        scheduler: Some(api::NodeScheduler {
            similarity: None,
            resource: api::node_scheduler::ResourceAffinity::MostResources.into(),
        }),
        allow_ips: vec![],
        deny_ips: vec![],
    };
    let status = tester.send_admin(Service::create, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_update() {
    let tester = super::Tester::new().await;
    let node = tester.node().await;
    let req = api::UpdateNodeRequest {
        id: node.id.to_string(),
        version: Some("10".to_string()),
        self_update: Some(false),
        container_status: None,
        address: Some("My main noderoni".to_string()),
        allow_ips: vec![],
        deny_ips: vec![],
    };
    tester.send_admin(Service::update, req).await.unwrap();
}

#[tokio::test]
async fn responds_internal_with_invalid_data_for_update() {
    let tester = super::Tester::new().await;
    let req = api::UpdateNodeRequest {
        // This is an invalid uuid so the api call should fail.
        id: "wowowow".to_string(),
        version: Some("stri-bu".to_string()),
        ..Default::default()
    };
    let status = tester.send_admin(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn responds_not_found_with_invalid_id_for_update() {
    let tester = super::Tester::new().await;
    let req = api::UpdateNodeRequest {
        // This uuid will not exist, so the api call should fail.
        id: uuid::Uuid::new_v4().to_string(),
        version: Some("stri-bu".to_string()),
        ..Default::default()
    };
    let status = tester.send_admin(Service::update, req).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound, "{status:?}");
}

#[tokio::test]
async fn responds_ok_with_valid_data_for_delete() {
    let tester = super::Tester::new().await;
    let node = tester.node().await;
    let req = api::DeleteNodeRequest {
        id: node.id.to_string(),
    };
    tester.send_admin(Service::delete, req).await.unwrap();
}
