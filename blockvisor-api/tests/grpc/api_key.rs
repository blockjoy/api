use blockvisor_api::auth::resource::{ResourceEntry, ResourceType};
use blockvisor_api::grpc::{api, common};
use blockvisor_api::models::api_key::ApiKey;
use blockvisor_api::models::org::Org;
use blockvisor_api::util::NanosUtc;
use tonic::transport::Channel;
use uuid::Uuid;

use crate::setup::helper::rpc;
use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type ApiKeyService = api::api_key_service_client::ApiKeyServiceClient<Channel>;
type OrgService = api::org_service_client::OrgServiceClient<Channel>;

#[tokio::test]
async fn user_can_create_and_list_api_keys() {
    let mut test = TestServer::new().await;
    let user1 = rpc::new_seed_user(&mut test).await;
    let user2 = rpc::new_seed_user(&mut test).await;

    // user1.jwt can create keys for user1.user_id
    let label1 = &test.rand_string(8).await;
    let created1 =
        rpc::create_api_key(&test, &user1.jwt, label1, ResourceType::User, user1.user_id)
            .await
            .unwrap();
    let token1 = created1.api_key.unwrap();

    // user1.jwt cannot create keys for user2
    let result =
        rpc::create_api_key(&test, &user1.jwt, label1, ResourceType::User, user2.user_id).await;
    assert!(result.is_err());

    // user2.jwt can create keys for user2
    let label2 = &test.rand_string(8).await;
    let created2 =
        rpc::create_api_key(&test, &user2.jwt, label2, ResourceType::User, user2.user_id)
            .await
            .unwrap();
    let token2 = created2.api_key.unwrap();
    assert!(token1 != token2);
    assert!(label1 != label2);

    // token1 can list keys for user1
    let keys1 = list_api_keys(&test, &token1).await.unwrap().api_keys;
    assert_eq!(keys1.len(), 1);
    assert_eq!(keys1[0].label.as_ref().unwrap(), label1);
    assert_eq!(
        keys1[0].created_at.clone().unwrap(),
        created1.created_at.unwrap()
    );

    // token2 can list keys for user2
    let keys2 = list_api_keys(&test, &token2).await.unwrap().api_keys;
    assert_eq!(keys2.len(), 1);
    assert_eq!(keys2[0].label.as_ref().unwrap(), label2);
}

#[tokio::test]
async fn user_can_update_label() {
    let mut test = TestServer::new().await;
    let key1 = rpc::new_seed_api_key(&mut test).await;
    let key2 = rpc::new_seed_api_key(&mut test).await;

    let keys = list_api_keys(&test, &key1.token).await.unwrap().api_keys;
    let key_id = keys[0].id.clone().unwrap();
    assert!(keys[0].updated_at.is_none());

    // key1.token can update key_id label
    let updated = update(&test, &key1.token, &key_id, Some("after"), None)
        .await
        .unwrap();
    let updated_at = updated.updated_at.unwrap();

    let keys = list_api_keys(&test, &key1.token).await.unwrap().api_keys;
    assert_eq!(keys[0].label.as_ref().unwrap(), "after");
    assert_eq!(keys[0].updated_at.clone().unwrap(), updated_at);

    // key2.token cannot update key_id label
    let result = update(&test, &key2.token, &key_id, Some("nope"), None).await;
    assert!(result.is_err())
}

#[tokio::test]
async fn user_can_update_scope() {
    let mut test = TestServer::new().await;
    let key1 = rpc::new_seed_api_key(&mut test).await;
    let key2 = rpc::new_seed_api_key(&mut test).await;

    // current resource is user
    let keys = list_api_keys(&test, &key1.token).await.unwrap().api_keys;
    let key_id = keys[0].id.clone().unwrap();
    let scope = keys[0].scope.clone().unwrap();
    assert_eq!(scope.resource(), common::Resource::User);

    // key2.token cannot update key_id resource
    let org_id = Uuid::new_v4().into();
    let entry = ResourceEntry::new_org(org_id);
    let result = update(&test, &key2.token, &key_id, None, Some(entry)).await;
    assert!(result.is_err());

    // key1.token can update key_id resource
    let updated = update(&test, &key1.token, &key_id, None, Some(entry))
        .await
        .unwrap();
    let updated_at: NanosUtc = updated.updated_at.unwrap().try_into().unwrap();

    // key1.token can no longer list api keys
    let result = list_api_keys(&test, &key1.token).await;
    assert!(result.is_err());

    // resource is now org
    let conn = &mut test.conn().await;
    let key = ApiKey::by_id(key_id.parse().unwrap(), conn).await.unwrap();
    assert_eq!(NanosUtc::from(key.updated_at.unwrap()), updated_at);
    assert_eq!(key.resource, ResourceType::Org);
    assert_eq!(*key.resource_id, *org_id);
}

#[tokio::test]
async fn user_can_regenerate_their_api_key() {
    let mut test = TestServer::new().await;
    let key1 = rpc::new_seed_api_key(&mut test).await;
    let key2 = rpc::new_seed_api_key(&mut test).await;

    let keys = list_api_keys(&test, &key1.token).await.unwrap().api_keys;
    let key_id = keys[0].id.clone().unwrap();

    // key2.token cannot regenerate key_id token
    let result = regenerate(&test, &key2.token, &key_id).await;
    assert!(result.is_err());

    // key1.token can regenerate key_id token
    let regenerated = regenerate(&test, &key1.token, &key_id).await.unwrap();
    let new_token = regenerated.api_key.unwrap();

    // key1.token is no longer valid
    assert!(list_api_keys(&test, &key1.token).await.is_err());

    // new_token is now valid
    let keys = list_api_keys(&test, &new_token).await.unwrap().api_keys;
    assert_eq!(keys[0].id.clone().unwrap(), key_id);
}

#[tokio::test]
async fn user_can_delete_their_api_key() {
    let mut test = TestServer::new().await;
    let key1 = rpc::new_seed_api_key(&mut test).await;
    let key2 = rpc::new_seed_api_key(&mut test).await;

    let keys = list_api_keys(&test, &key1.token).await.unwrap().api_keys;
    let key_id = keys[0].id.clone().unwrap();

    // key2.token cannot delete key_id
    assert!(delete(&test, &key2.token, &key_id).await.is_err());

    // key1.token can delete key_id
    assert!(delete(&test, &key1.token, &key_id).await.is_ok());

    // key1.token is no longer valid
    assert!(list_api_keys(&test, &key1.token).await.is_err());
}

#[tokio::test]
async fn user_can_manage_org_with_api_key() {
    let mut test = TestServer::new().await;
    let key1 = rpc::new_seed_api_key(&mut test).await;
    let key2 = rpc::new_seed_api_key(&mut test).await;

    let name = test.rand_string(8).await;
    let req = api::OrgServiceCreateRequest { name: name.clone() };
    let created = test
        .send_with(OrgService::create, req, &key1.token)
        .await
        .unwrap();
    let org = created.org.unwrap();
    let org_id = org.id;
    assert_eq!(org.name, name);

    // key2.token cannot get org_id
    let req = api::OrgServiceGetRequest { id: org_id.clone() };
    let result = test.send_with(OrgService::get, req, &key2.token).await;
    assert!(result.is_err());

    // key1.token can get org_id
    let req = api::OrgServiceGetRequest { id: org_id.clone() };
    let result = test.send_with(OrgService::get, req, &key1.token).await;
    assert!(result.is_ok());

    // add key2.token as org member
    let conn = &mut test.conn().await;
    let org = Org::by_id(org_id.parse().unwrap(), conn).await.unwrap();
    org.add_member(key2.user_id, conn).await.unwrap();

    // key2.token can now get org_id
    let req = api::OrgServiceGetRequest { id: org_id.clone() };
    let result = test.send_with(OrgService::get, req, &key2.token).await;
    assert!(result.is_ok());

    // member key2.token cannot delete org_id
    let req = api::OrgServiceDeleteRequest { id: org_id.clone() };
    let result = test.send_with(OrgService::delete, req, &key2.token).await;
    assert!(result.is_err());

    // owner key1.token can delete org_id
    let req = api::OrgServiceDeleteRequest { id: org_id };
    let result = test.send_with(OrgService::delete, req, &key1.token).await;
    assert!(result.is_ok());
}

async fn list_api_keys(
    test: &TestServer,
    token: &str,
) -> Result<api::ApiKeyServiceListResponse, tonic::Status> {
    let req = api::ApiKeyServiceListRequest {};
    test.send_with(ApiKeyService::list, req, token).await
}

async fn update(
    test: &TestServer,
    token: &str,
    key_id: &str,
    new_label: Option<&str>,
    new_resource_entry: Option<ResourceEntry>,
) -> Result<api::ApiKeyServiceUpdateResponse, tonic::Status> {
    let req = api::ApiKeyServiceUpdateRequest {
        id: key_id.into(),
        label: new_label.map(Into::into),
        scope: new_resource_entry.map(api::ApiKeyScope::from_entry),
    };

    test.send_with(ApiKeyService::update, req, token).await
}

async fn regenerate(
    test: &TestServer,
    token: &str,
    key_id: &str,
) -> Result<api::ApiKeyServiceRegenerateResponse, tonic::Status> {
    let req = api::ApiKeyServiceRegenerateRequest { id: key_id.into() };
    test.send_with(ApiKeyService::regenerate, req, token).await
}

async fn delete(
    test: &TestServer,
    token: &str,
    key_id: &str,
) -> Result<api::ApiKeyServiceDeleteResponse, tonic::Status> {
    let req = api::ApiKeyServiceDeleteRequest { id: key_id.into() };
    test.send_with(ApiKeyService::delete, req, token).await
}
