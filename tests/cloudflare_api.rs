mod setup;

use blockvisor_api::cloudflare::CloudflareApi;
use blockvisor_api::grpc::{api, api::node_service_client};
use blockvisor_api::models;
use tonic::transport;

type Service = node_service_client::NodeServiceClient<transport::Channel>;

#[tokio::test]
async fn can_create_node_dns() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let api = CloudflareApi::new("127.0.0.1".to_string())?;
    let mut name = String::from("test_");
    name.push_str(petname::petname(3, "_").as_str());
    let id = api.get_node_dns(name, "127.0.0.1".to_string()).await?;

    assert!(!id.is_empty());

    Ok(())
}

#[tokio::test]
async fn can_create_node_with_dns() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let mut conn = tester.conn().await;
    let blockchain = tester.blockchain().await;
    let user = tester.admin_user().await;
    let org = tester.org_for(&user).await;
    let req = api::NodeServiceCreateRequest {
        org_id: org.id.to_string(),
        blockchain_id: blockchain.id.to_string(),
        node_type: api::NodeType::Validator.into(),
        properties: vec![],
        version: "3.3.0".to_string(),
        network: "some network".to_string(),
        placement: Some(api::NodePlacement {
            placement: Some(api::node_placement::Placement::HostId(
                tester.host().await.id.to_string(),
            )),
        }),
        allow_ips: vec![],
        deny_ips: vec![],
    };

    tester.send_admin(Service::create, req).await.unwrap();
    let nodes = models::Node::find_all_by_org(org.id, 0, 1, &mut conn).await?;
    let node = nodes.first().unwrap();

    assert!(!node.dns_record_id.is_empty());

    Ok(())
}

#[tokio::test]
#[ignore]
async fn can_remove_node_dns() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let api = CloudflareApi::new("127.0.0.1".to_string())?;
    let id = "b32dfad93146bf7593b258e3064642c0".to_string();

    assert!(api.remove_node_dns(id).await.is_ok());

    Ok(())
}
