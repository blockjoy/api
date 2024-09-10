use std::collections::{HashMap, HashSet};

use blockvisor_api::database::seed::{
    ARCHIVE_ID_1, IMAGE_ID, ORG_ID, ORG_IMAGE_ID, ORG_PROTOCOL_KEY, ORG_VARIANT_KEY, PROTOCOL_KEY,
    PROTOCOL_VERSION_ID, SEMANTIC_VERSION, STORE_ID_1, STORE_ID_2, VARIANT_KEY,
};
use blockvisor_api::grpc::{api, common};
use blockvisor_api::model::image::rule::{FirewallAction, FirewallDirection};
use tonic::Code;
use uuid::Uuid;

use crate::setup::helper::traits::{ImageService, SocketRpc};
use crate::setup::TestServer;

#[tokio::test]
async fn add_a_new_image() {
    use blockvisor_api::model::image::rule::{FirewallAction::*, FirewallDirection::*};

    let test = TestServer::new().await;
    let req = api::ImageServiceAddImageRequest {
        protocol_version_id: PROTOCOL_VERSION_ID.into(),
        org_id: Some(ORG_ID.into()),
        image_uri: "docker:TODO".to_string(),
        description: None,
        properties: vec![
            add_image_property("prop1", false),
            add_image_property("prop2", true),
            add_image_property("prop3", false),
            add_image_property("prop4", true),
        ],
        firewall: Some(common::FirewallConfig {
            default_in: common::FirewallAction::Drop.into(),
            default_out: common::FirewallAction::Allow.into(),
            rules: vec![
                firewall_rule("rule1", Inbound, Drop, None, None),
                firewall_rule("rule2", Outbound, Allow, Some("1.2.3.4"), Some(1337)),
            ],
        }),
        min_cpu_cores: 1,
        min_memory_bytes: 2,
        min_disk_bytes: 3,
        ramdisks: vec![],
        archive_pointers: vec![
            archive_pointer(vec![], Some("default-store-id")),
            archive_pointer(vec!["prop2"], Some("prop2-store-id")),
            archive_pointer(vec!["prop4"], Some("prop4-store-id")),
            archive_pointer(vec!["prop2", "prop4"], None),
        ],
    };

    // an org admin can't add new images
    let result = test.send_admin(ImageService::add_image, req.clone()).await;
    assert_eq!(result.unwrap_err().code(), Code::PermissionDenied);

    // a blockjoy admin can add new images
    let resp = test.send_super(ImageService::add_image, req).await.unwrap();
    let image = resp.image.unwrap();
    assert_eq!(image.protocol_version_id, PROTOCOL_VERSION_ID);
    assert_eq!(image.build_version, 2);

    let firewall = image.firewall.unwrap();
    let rules: HashSet<_> = firewall.rules.into_iter().map(|r| r.key).collect();
    assert_eq!(rules.len(), 2);
    assert!(rules.contains("rule1"));
    assert!(rules.contains("rule2"));

    let id_to_key: HashMap<_, _> = image
        .properties
        .into_iter()
        .filter(|property| property.new_archive)
        .map(|property| (property.image_property_id, property.key))
        .collect();

    assert_eq!(resp.archives.len(), 3);
    for archive in resp.archives {
        let keys = archive
            .image_property_ids
            .iter()
            .map(|id| id_to_key.get(id).unwrap())
            .collect::<Vec<_>>();
        match keys.as_slice() {
            [] => assert_eq!(archive.store_id, "default-store-id"),
            [key] if *key == "prop2" => assert_eq!(archive.store_id, "prop2-store-id"),
            [key] if *key == "prop4" => assert_eq!(archive.store_id, "prop4-store-id"),
            _ => panic!("unexpected archive: {archive:?}"),
        }
    }
}

fn add_image_property<S: Into<String>>(key: S, new_archive: bool) -> api::AddImageProperty {
    api::AddImageProperty {
        key: key.into(),
        key_group: None,
        is_group_default: None,
        new_archive,
        default_value: "default".into(),
        dynamic_value: false,
        description: None,
        ui_type: common::UiType::Text.into(),
        add_cpu_cores: None,
        add_memory_bytes: None,
        add_disk_bytes: None,
    }
}

fn firewall_rule<S: Into<String>>(
    key: S,
    direction: FirewallDirection,
    action: FirewallAction,
    ip: Option<S>,
    port: Option<u32>,
) -> common::FirewallRule {
    let key = key.into();
    common::FirewallRule {
        key: key.clone(),
        description: None,
        protocol: common::FirewallProtocol::Tcp.into(),
        direction: common::FirewallDirection::from(direction).into(),
        action: common::FirewallAction::from(action).into(),
        ips: ip
            .map(|ip| {
                let ip = ip.into();
                vec![common::IpName { ip, name: None }]
            })
            .unwrap_or_default(),
        ports: port
            .map(|port| vec![common::PortName { port, name: None }])
            .unwrap_or_default(),
    }
}

fn archive_pointer<S: Into<String>>(keys: Vec<S>, store_id: Option<S>) -> api::ArchivePointer {
    api::ArchivePointer {
        new_archive_keys: keys.into_iter().map(Into::into).collect(),
        pointer: Some(if let Some(id) = store_id {
            api::archive_pointer::Pointer::StoreId(id.into())
        } else {
            api::archive_pointer::Pointer::Disallowed(())
        }),
    }
}

#[tokio::test]
async fn get_an_existing_image() {
    let test = TestServer::new().await;

    // can't find unknown protocol key
    let req = api::ImageServiceGetImageRequest {
        version_key: version_key("unknown", VARIANT_KEY),
        org_id: Some(ORG_ID.into()),
        semantic_version: None,
        build_version: None,
    };
    let result = test.send_member(ImageService::get_image, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can't find org-specific image without org_id
    let req = api::ImageServiceGetImageRequest {
        version_key: version_key(ORG_PROTOCOL_KEY, ORG_VARIANT_KEY),
        org_id: None,
        semantic_version: None,
        build_version: None,
    };
    let result = test.send_member(ImageService::get_image, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can find latest image
    let req = api::ImageServiceGetImageRequest {
        version_key: version_key(PROTOCOL_KEY, VARIANT_KEY),
        org_id: Some(ORG_ID.into()),
        semantic_version: None,
        build_version: None,
    };
    let result = test.send_member(ImageService::get_image, req).await;
    let image = result.unwrap().image.unwrap();
    assert_eq!(image.image_id, IMAGE_ID);

    // can't find unknown version
    let req = api::ImageServiceGetImageRequest {
        version_key: version_key(PROTOCOL_KEY, VARIANT_KEY),
        org_id: Some(ORG_ID.into()),
        semantic_version: Some("9.8.7".into()),
        build_version: None,
    };
    let result = test.send_member(ImageService::get_image, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can find known protocol
    let req = api::ImageServiceGetImageRequest {
        version_key: version_key(PROTOCOL_KEY, VARIANT_KEY),
        org_id: None,
        semantic_version: Some(SEMANTIC_VERSION.into()),
        build_version: None,
    };
    let result = test.send_member(ImageService::get_image, req).await;
    let image = result.unwrap().image.unwrap();
    assert_eq!(image.image_id, IMAGE_ID);

    // can't find unknown build for latest image
    let req = api::ImageServiceGetImageRequest {
        version_key: version_key(PROTOCOL_KEY, VARIANT_KEY),
        org_id: None,
        semantic_version: None,
        build_version: Some(999),
    };
    let result = test.send_member(ImageService::get_image, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can find known image build
    let req = api::ImageServiceGetImageRequest {
        version_key: version_key(PROTOCOL_KEY, VARIANT_KEY),
        org_id: None,
        semantic_version: None,
        build_version: Some(1),
    };
    let result = test.send_member(ImageService::get_image, req).await;
    let image = result.unwrap().image.unwrap();
    assert_eq!(image.image_id, IMAGE_ID);
}

#[tokio::test]
async fn list_archives() {
    let test = TestServer::new().await;

    // can't find unknown image_id
    let req = api::ImageServiceListArchivesRequest {
        image_id: Uuid::new_v4().to_string(),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ImageService::list_archives, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can't find org-specific image without org_id
    let req = api::ImageServiceListArchivesRequest {
        image_id: ORG_IMAGE_ID.into(),
        org_id: None,
    };
    let result = test.send_member(ImageService::list_archives, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can list public archives for an image
    let req = api::ImageServiceListArchivesRequest {
        image_id: IMAGE_ID.into(),
        org_id: None,
    };
    let result = test.send_member(ImageService::list_archives, req).await;
    assert_eq!(result.unwrap().archives.len(), 2);

    // can also list org archives for an image
    let req = api::ImageServiceListArchivesRequest {
        image_id: ORG_IMAGE_ID.into(),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ImageService::list_archives, req).await;
    assert_eq!(result.unwrap().archives.len(), 0);
}

#[tokio::test]
async fn update_existing_archive() {
    let test = TestServer::new().await;

    let list = api::ImageServiceListArchivesRequest {
        image_id: IMAGE_ID.into(),
        org_id: Some(ORG_ID.into()),
    };
    let new_store_id = "new-store-id";
    let update = api::ImageServiceUpdateArchiveRequest {
        archive_id: ARCHIVE_ID_1.to_string(),
        store_id: Some(new_store_id.into()),
    };

    // check existing store ids
    let result = test
        .send_member(ImageService::list_archives, list.clone())
        .await;
    let archives = result.unwrap().archives;
    let store_ids: HashSet<_> = archives.iter().map(|a| a.store_id.as_str()).collect();
    assert_eq!(store_ids.len(), 2);
    assert!(store_ids.contains(&STORE_ID_1));
    assert!(store_ids.contains(&STORE_ID_2));

    // org admin can't update an archive
    let result = test
        .send_admin(ImageService::update_archive, update.clone())
        .await;
    assert_eq!(result.unwrap_err().code(), Code::PermissionDenied);

    // super user can update an archive
    let result = test.send_super(ImageService::update_archive, update).await;
    assert_eq!(result.unwrap().archive.unwrap().archive_id, ARCHIVE_ID_1);

    // check new store ids
    let result = test
        .send_member(ImageService::list_archives, list.clone())
        .await;
    let archives = result.unwrap().archives;
    let store_ids: HashSet<_> = archives.iter().map(|a| a.store_id.as_str()).collect();
    assert_eq!(store_ids.len(), 2);
    assert!(store_ids.contains(&new_store_id));
    assert!(store_ids.contains(&STORE_ID_2));
}

#[tokio::test]
async fn update_existing_image() {
    let test = TestServer::new().await;

    let get_image = |org_id: Option<&str>| api::ImageServiceGetImageRequest {
        version_key: version_key(ORG_PROTOCOL_KEY, VARIANT_KEY),
        org_id: org_id.map(Into::into),
        semantic_version: None,
        build_version: None,
    };
    let update = api::ImageServiceUpdateImageRequest {
        image_id: ORG_IMAGE_ID.to_string(),
        visibility: Some(common::Visibility::Private.into()),
    };

    // org member can get latest org image
    let req = get_image(Some(ORG_ID));
    let result = test.send_member(ImageService::get_image, req).await;
    let image = result.unwrap().image.unwrap();
    assert_eq!(image.image_id, ORG_IMAGE_ID);
    assert_eq!(image.visibility(), common::Visibility::Public);

    // no org can't find org image
    let req = get_image(None);
    let result = test.send_member(ImageService::get_image, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // org admin can't change visibility
    let result = test
        .send_admin(ImageService::update_image, update.clone())
        .await;
    assert_eq!(result.unwrap_err().code(), Code::PermissionDenied);

    // super user can change visibility
    let result = test.send_super(ImageService::update_image, update).await;
    let image = result.unwrap().image.unwrap();
    assert_eq!(image.image_id, ORG_IMAGE_ID);
    assert_eq!(image.visibility(), common::Visibility::Private);

    // org member can't find private visibility image
    let req = get_image(Some(ORG_ID));
    let result = test.send_member(ImageService::get_image, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);
}

fn version_key(protocol_key: &str, variant_key: &str) -> Option<common::ProtocolVersionKey> {
    Some(common::ProtocolVersionKey {
        protocol_key: protocol_key.into(),
        variant_key: variant_key.into(),
    })
}
