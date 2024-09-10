use blockvisor_api::database::seed::{
    ORG_ID, ORG_PROTOCOL_ID, ORG_PROTOCOL_KEY, ORG_PROTOCOL_VERSION_ID, ORG_SEMANTIC_VERSION,
    PROTOCOL_ID, PROTOCOL_KEY, PROTOCOL_VERSION_ID, VARIANT_KEY,
};
use blockvisor_api::grpc::api::protocol_service_get_protocol_request::Protocol as ApiProtocol;
use blockvisor_api::grpc::{api, common};
use tonic::Code;
use uuid::Uuid;

use crate::setup::helper::traits::{ProtocolService, SocketRpc};
use crate::setup::TestServer;

#[tokio::test]
async fn add_a_new_protocol() {
    let test = TestServer::new().await;
    let (key, name, ticker) = ("sui", "Sui", "SUI");
    let req = api::ProtocolServiceAddProtocolRequest {
        key: key.to_string(),
        name: name.to_string(),
        org_id: None,
        description: None,
        ticker: Some(ticker.to_string()),
    };

    // an org admin can't add new protocols
    let result = test
        .send_admin(ProtocolService::add_protocol, req.clone())
        .await;
    assert_eq!(result.unwrap_err().code(), Code::PermissionDenied);

    // a blockjoy admin can add new protocols
    let resp = test
        .send_super(ProtocolService::add_protocol, req)
        .await
        .unwrap();
    let protocol = resp.protocol.unwrap();
    dbg!(&protocol);
    assert_eq!(protocol.key, key);
    assert_eq!(protocol.name, name);
    assert!(protocol.org_id.is_none());
    assert!(protocol.description.is_none());
    assert_eq!(protocol.ticker.unwrap(), ticker);

    // an org member can't view a private visibility protocol
    let req = api::ProtocolServiceGetProtocolRequest {
        protocol: Some(ApiProtocol::ProtocolId(protocol.protocol_id.to_string())),
        org_id: None,
    };
    let result = test
        .send_member(ProtocolService::get_protocol, req.clone())
        .await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // a super user can view a private visibility protocol
    let resp = test
        .send_super(ProtocolService::get_protocol, req)
        .await
        .unwrap();
    assert_eq!(resp.protocol.unwrap().name, name);
}

#[tokio::test]
async fn add_a_new_version() {
    let test = TestServer::new().await;
    let version = "2.0.0";

    let add_version = |org_id: Option<&str>, version: &str| api::ProtocolServiceAddVersionRequest {
        org_id: org_id.map(|id| id.to_string()),
        version_key: Some(common::ProtocolVersionKey {
            protocol_key: ORG_PROTOCOL_KEY.into(),
            variant_key: VARIANT_KEY.into(),
        }),
        semantic_version: version.to_string(),
        sku_code: "TN".to_string(),
        description: None,
    };

    // can't add a new version for an org protocol without org_id
    let req = add_version(None, version);
    let result = test.send_super(ProtocolService::add_version, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can't add a new version for a bad semantic version
    let req = add_version(Some(ORG_ID), "2-0-0");
    let result = test.send_super(ProtocolService::add_version, req).await;
    assert_eq!(result.unwrap_err().code(), Code::InvalidArgument);

    // an org admin can't add a new version
    let req = add_version(Some(ORG_ID), version);
    let result = test.send_admin(ProtocolService::add_version, req).await;
    assert_eq!(result.unwrap_err().code(), Code::PermissionDenied);

    // a super user can add a new version
    let req = add_version(Some(ORG_ID), version);
    let resp = test
        .send_super(ProtocolService::add_version, req)
        .await
        .unwrap();
    assert_eq!(resp.version.unwrap().semantic_version, version);
}

#[tokio::test]
async fn get_an_existing_protocol() {
    let test = TestServer::new().await;

    // can't find unknown protocol_id
    let req = api::ProtocolServiceGetProtocolRequest {
        protocol: Some(ApiProtocol::ProtocolId(Uuid::new_v4().to_string())),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ProtocolService::get_protocol, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can't find org-specific protocol without org_id
    let req = api::ProtocolServiceGetProtocolRequest {
        protocol: Some(ApiProtocol::ProtocolId(ORG_PROTOCOL_ID.to_string())),
        org_id: None,
    };
    let result = test.send_member(ProtocolService::get_protocol, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can't find org-specifc protocol if not an org member
    let req = api::ProtocolServiceGetProtocolRequest {
        protocol: Some(ApiProtocol::ProtocolId(ORG_PROTOCOL_ID.to_string())),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_unknown(ProtocolService::get_protocol, req).await;
    assert_eq!(result.unwrap_err().code(), Code::PermissionDenied);

    // org member can find org protocol
    let req = api::ProtocolServiceGetProtocolRequest {
        protocol: Some(ApiProtocol::ProtocolId(ORG_PROTOCOL_ID.to_string())),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ProtocolService::get_protocol, req).await;
    let protocol = result.unwrap().protocol.unwrap();
    assert_eq!(protocol.protocol_id, ORG_PROTOCOL_ID);
    assert_eq!(protocol.key, ORG_PROTOCOL_KEY);
}

#[tokio::test]
async fn get_latest_version() {
    let test = TestServer::new().await;

    // can't find unknown protocol_id
    let req = api::ProtocolServiceGetLatestRequest {
        version_key: Some(common::ProtocolVersionKey {
            protocol_key: Uuid::new_v4().to_string(),
            variant_key: VARIANT_KEY.into(),
        }),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ProtocolService::get_latest, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can't find org-specific protocol without org_id
    let req = api::ProtocolServiceGetLatestRequest {
        version_key: Some(common::ProtocolVersionKey {
            protocol_key: ORG_PROTOCOL_KEY.into(),
            variant_key: VARIANT_KEY.into(),
        }),
        org_id: None,
    };
    let result = test.send_member(ProtocolService::get_latest, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);

    // can get latest version
    let req = api::ProtocolServiceGetLatestRequest {
        version_key: Some(common::ProtocolVersionKey {
            protocol_key: ORG_PROTOCOL_KEY.into(),
            variant_key: VARIANT_KEY.into(),
        }),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ProtocolService::get_latest, req).await;
    let version = result.unwrap().protocol_version.unwrap();
    assert_eq!(version.protocol_version_id, ORG_PROTOCOL_VERSION_ID);
    assert_eq!(version.semantic_version, ORG_SEMANTIC_VERSION);
}

#[tokio::test]
async fn list_existing_protocols() {
    let test = TestServer::new().await;
    let list = |org_ids| api::ProtocolServiceListProtocolsRequest {
        org_ids,
        offset: 0,
        limit: 2,
        search: None,
        sort: vec![],
    };

    // public protocols without org id
    let req = list(vec![]);
    let resp = test
        .send_member(ProtocolService::list_protocols, req)
        .await
        .unwrap();
    assert_eq!(resp.protocols.len(), 1);

    // denied when listing for org without membership
    let req = list(vec![ORG_ID.to_string()]);
    let result = test
        .send_unknown(ProtocolService::list_protocols, req)
        .await;
    assert_eq!(result.unwrap_err().code(), Code::PermissionDenied);

    // includes private protocols with an org id
    let req = list(vec![ORG_ID.to_string()]);
    let resp = test
        .send_member(ProtocolService::list_protocols, req)
        .await
        .unwrap();
    assert_eq!(resp.protocols.len(), 2);
}

#[tokio::test]
async fn list_existing_versions() {
    let test = TestServer::new().await;

    // empty list when missing org id
    let req = api::ProtocolServiceListVersionsRequest {
        version_key: Some(common::ProtocolVersionKey {
            protocol_key: ORG_PROTOCOL_KEY.into(),
            variant_key: VARIANT_KEY.into(),
        }),
        org_id: None,
    };
    let result = test.send_member(ProtocolService::list_versions, req).await;
    assert_eq!(result.unwrap().protocol_versions.len(), 0);

    // org member can list versions
    let req = api::ProtocolServiceListVersionsRequest {
        version_key: Some(common::ProtocolVersionKey {
            protocol_key: ORG_PROTOCOL_KEY.into(),
            variant_key: VARIANT_KEY.into(),
        }),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ProtocolService::list_versions, req).await;
    let versions = result.unwrap().protocol_versions;
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].protocol_version_id, ORG_PROTOCOL_VERSION_ID);
}

#[tokio::test]
async fn update_an_existing_protocol() {
    let test = TestServer::new().await;

    // org member can find public visibility protocol
    let req = api::ProtocolServiceGetProtocolRequest {
        protocol: Some(ApiProtocol::ProtocolId(PROTOCOL_ID.to_string())),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ProtocolService::get_protocol, req).await;
    let protocol = result.unwrap().protocol.unwrap();
    assert_eq!(protocol.protocol_id, PROTOCOL_ID);
    assert_eq!(protocol.visibility(), common::Visibility::Public);

    // org admin can't change visibility
    let req = api::ProtocolServiceUpdateProtocolRequest {
        protocol_id: PROTOCOL_ID.to_string(),
        name: None,
        description: None,
        visibility: Some(common::Visibility::Private.into()),
    };
    let result = test.send_admin(ProtocolService::update_protocol, req).await;
    assert_eq!(result.unwrap_err().code(), Code::PermissionDenied);

    // super user can change visibility
    let req = api::ProtocolServiceUpdateProtocolRequest {
        protocol_id: PROTOCOL_ID.to_string(),
        name: None,
        description: None,
        visibility: Some(common::Visibility::Private.into()),
    };
    let result = test.send_super(ProtocolService::update_protocol, req).await;
    let protocol = result.unwrap().protocol.unwrap();
    assert_eq!(protocol.protocol_id, PROTOCOL_ID);
    assert_eq!(protocol.visibility(), common::Visibility::Private);

    // org member can't find private visibility protocol
    let req = api::ProtocolServiceGetProtocolRequest {
        protocol: Some(ApiProtocol::ProtocolId(PROTOCOL_ID.to_string())),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ProtocolService::get_protocol, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);
}

#[tokio::test]
async fn update_existing_version() {
    let test = TestServer::new().await;

    // org member can find public visibility version
    let req = api::ProtocolServiceGetLatestRequest {
        version_key: Some(common::ProtocolVersionKey {
            protocol_key: PROTOCOL_KEY.into(),
            variant_key: VARIANT_KEY.into(),
        }),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ProtocolService::get_latest, req).await;
    let version = result.unwrap().protocol_version.unwrap();
    assert_eq!(version.protocol_version_id, PROTOCOL_VERSION_ID);
    assert_eq!(version.visibility(), common::Visibility::Public);

    // org admin can't change visibility
    let req = api::ProtocolServiceUpdateVersionRequest {
        protocol_version_id: PROTOCOL_VERSION_ID.to_string(),
        sku_code: None,
        description: None,
        visibility: Some(common::Visibility::Private.into()),
    };
    let result = test.send_admin(ProtocolService::update_version, req).await;
    assert_eq!(result.unwrap_err().code(), Code::PermissionDenied);

    // super user can change visibility
    let req = api::ProtocolServiceUpdateVersionRequest {
        protocol_version_id: PROTOCOL_VERSION_ID.to_string(),
        sku_code: None,
        description: None,
        visibility: Some(common::Visibility::Private.into()),
    };
    let result = test.send_super(ProtocolService::update_version, req).await;
    let version = result.unwrap().protocol_version.unwrap();
    assert_eq!(version.protocol_version_id, PROTOCOL_VERSION_ID);
    assert_eq!(version.visibility(), common::Visibility::Private);

    // org member can't find private visibility version
    let req = api::ProtocolServiceGetLatestRequest {
        version_key: Some(common::ProtocolVersionKey {
            protocol_key: PROTOCOL_KEY.into(),
            variant_key: VARIANT_KEY.into(),
        }),
        org_id: Some(ORG_ID.into()),
    };
    let result = test.send_member(ProtocolService::get_latest, req).await;
    assert_eq!(result.unwrap_err().code(), Code::NotFound);
}
