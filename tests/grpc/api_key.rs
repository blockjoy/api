use blockvisor_api::auth::claims::Claims;
use blockvisor_api::auth::resource::{ResourceEntry, UserId};
use blockvisor_api::auth::token::jwt;
use blockvisor_api::database::tests::seed;
use blockvisor_api::grpc::api;
use blockvisor_api::models::api_key::{ApiKey, ApiResource};
use blockvisor_api::models::org::{NewOrgUser, Org, OrgRole};
use blockvisor_api::models::user::{NewUser, User};
use blockvisor_api::timestamp::NanosUtc;
use tonic::transport::Channel;
use uuid::Uuid;

use super::Tester;

const TEST_PASSWORD: &str = "hunter2";

type ApiKeyService = api::api_key_service_client::ApiKeyServiceClient<Channel>;
type AuthService = api::auth_service_client::AuthServiceClient<Channel>;
type OrgService = api::org_service_client::OrgServiceClient<Channel>;

#[tokio::test]
async fn user_can_create_and_list_api_keys() {
    let mut test = Tester::new().await;
    let (user1, jwt1) = new_user(&mut test).await;
    let (user2, jwt2) = new_user(&mut test).await;

    // jwt1 can create keys for user1
    let label1 = &test.rand_string(8);
    let created1 = create_api_key(&test, &jwt1, label1, ApiResource::User, user1)
        .await
        .unwrap();
    let token1 = created1.api_key.unwrap();

    // jwt1 cannot create keys for user2
    let result = create_api_key(&test, &jwt1, label1, ApiResource::User, user2).await;
    assert!(result.is_err());

    // jwt2 can create keys for user2
    let label2 = &test.rand_string(8);
    let created2 = create_api_key(&test, &jwt2, label2, ApiResource::User, user2)
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
    let mut test = Tester::new().await;
    let (_, token1) = new_user_token(&mut test).await;
    let (_, token2) = new_user_token(&mut test).await;

    let keys = list_api_keys(&test, &token1).await.unwrap().api_keys;
    let key_id = keys[0].id.clone().unwrap();
    assert!(keys[0].updated_at.is_none());

    // token1 can update key_id label
    let updated = update(&test, &token1, &key_id, Some("after"), None)
        .await
        .unwrap();
    let updated_at = updated.updated_at.unwrap();

    let keys = list_api_keys(&test, &token1).await.unwrap().api_keys;
    assert_eq!(keys[0].label.as_ref().unwrap(), "after");
    assert_eq!(keys[0].updated_at.clone().unwrap(), updated_at);

    // token2 cannot update key_id label
    let result = update(&test, &token2, &key_id, Some("nope"), None).await;
    assert!(result.is_err())
}

#[tokio::test]
async fn user_can_update_scope() {
    let mut test = Tester::new().await;
    let (_, token1) = new_user_token(&mut test).await;
    let (_, token2) = new_user_token(&mut test).await;

    // current resource is user
    let keys = list_api_keys(&test, &token1).await.unwrap().api_keys;
    let key_id = keys[0].id.clone().unwrap();
    let scope = keys[0].scope.clone().unwrap();
    assert_eq!(scope.resource, ApiResource::User as i32);

    // token2 cannot update key_id resource
    let org_id = Uuid::new_v4().into();
    let entry = ResourceEntry::new_org(org_id);
    let result = update(&test, &token2, &key_id, None, Some(entry)).await;
    assert!(result.is_err());

    // token1 can update key_id resource
    let updated = update(&test, &token1, &key_id, None, Some(entry))
        .await
        .unwrap();
    let updated_at: NanosUtc = updated.updated_at.unwrap().try_into().unwrap();

    // token1 can no longer list api keys
    assert!(list_api_keys(&test, &token1).await.is_err());

    // resource is now org
    let conn = &mut test.conn().await;
    let key = ApiKey::find_by_id(key_id.parse().unwrap(), conn)
        .await
        .unwrap();
    assert_eq!(NanosUtc::from(key.updated_at.unwrap()), updated_at);
    assert_eq!(key.resource, ApiResource::Org);
    assert_eq!(*key.resource_id, *org_id);
}

#[tokio::test]
async fn user_can_regenerate_their_api_key() {
    let mut test = Tester::new().await;
    let (_, token1) = new_user_token(&mut test).await;
    let (_, token2) = new_user_token(&mut test).await;

    let keys = list_api_keys(&test, &token1).await.unwrap().api_keys;
    let key_id = keys[0].id.clone().unwrap();

    // token2 cannot regenerate key_id token
    let result = regenerate(&test, &token2, &key_id).await;
    assert!(result.is_err());

    // token1 can regenerate key_id token
    let regenerated = regenerate(&test, &token1, &key_id).await.unwrap();
    let new_token1 = regenerated.api_key.unwrap();

    // token1 is no longer valid
    assert!(list_api_keys(&test, &token1).await.is_err());

    // new_token1 is now valid
    let keys = list_api_keys(&test, &new_token1).await.unwrap().api_keys;
    assert_eq!(keys[0].id.clone().unwrap(), key_id);
}

#[tokio::test]
async fn user_can_delete_their_api_key() {
    let mut test = Tester::new().await;
    let (_, token1) = new_user_token(&mut test).await;
    let (_, token2) = new_user_token(&mut test).await;

    let keys = list_api_keys(&test, &token1).await.unwrap().api_keys;
    let key_id = keys[0].id.clone().unwrap();

    // token2 cannot delete key_id
    assert!(delete(&test, &token2, &key_id).await.is_err());

    // token1 can delete key_id
    assert!(delete(&test, &token1, &key_id).await.is_ok());

    // token1 is no longer valid
    assert!(list_api_keys(&test, &token1).await.is_err());
}

#[tokio::test]
async fn user_can_manage_org_with_api_key() {
    let mut test = Tester::new().await;
    let (_, token1) = new_user_token(&mut test).await;
    let (user2_id, token2) = new_user_token(&mut test).await;

    let name = test.rand_string(8);
    let req = api::OrgServiceCreateRequest { name: name.clone() };
    let created = test
        .send_with(OrgService::create, req, &token1)
        .await
        .unwrap();
    let org = created.org.unwrap();
    let org_id = org.id;
    assert_eq!(org.name, name);

    // token2 cannot get org_id
    let req = api::OrgServiceGetRequest { id: org_id.clone() };
    let result = test.send_with(OrgService::get, req, &token2).await;
    assert!(result.is_err());

    // token1 can get org_id
    let req = api::OrgServiceGetRequest { id: org_id.clone() };
    let result = test.send_with(OrgService::get, req, &token1).await;
    assert!(result.is_ok());

    // add token2 as org member
    let conn = &mut test.conn().await;
    let org = Org::find_by_id(org_id.parse().unwrap(), conn)
        .await
        .unwrap();
    org.add_member(user2_id, OrgRole::Member, conn)
        .await
        .unwrap();

    // token2 can now get org_id
    let req = api::OrgServiceGetRequest { id: org_id.clone() };
    let result = test.send_with(OrgService::get, req, &token2).await;
    assert!(result.is_ok());

    // member token2 cannot delete org_id
    let req = api::OrgServiceDeleteRequest { id: org_id.clone() };
    let result = test.send_with(OrgService::delete, req, &token2).await;
    assert!(result.is_err());

    // owner token1 can delete org_id
    let req = api::OrgServiceDeleteRequest { id: org_id };
    let result = test.send_with(OrgService::delete, req, &token1).await;
    assert!(result.is_ok());
}

async fn create_api_key<U: Into<Uuid>>(
    test: &Tester,
    token: &str,
    label: &str,
    resource: ApiResource,
    resource_id: U,
) -> Result<api::CreateApiKeyResponse, tonic::Status> {
    let scope = api::ApiKeyScope {
        resource: resource as i32,
        resource_id: Some(resource_id.into().to_string()),
    };

    let req = api::CreateApiKeyRequest {
        label: label.to_string(),
        scope: Some(scope),
    };

    test.send_with(ApiKeyService::create, req, token).await
}

async fn list_api_keys(
    test: &Tester,
    token: &str,
) -> Result<api::ListApiKeyResponse, tonic::Status> {
    let req = api::ListApiKeyRequest {};
    test.send_with(ApiKeyService::list, req, token).await
}

async fn update(
    test: &Tester,
    token: &str,
    key_id: &str,
    new_label: Option<&str>,
    new_resource_entry: Option<ResourceEntry>,
) -> Result<api::UpdateApiKeyResponse, tonic::Status> {
    let req = api::UpdateApiKeyRequest {
        id: key_id.into(),
        label: new_label.map(Into::into),
        scope: new_resource_entry.map(api::ApiKeyScope::from_entry),
    };

    test.send_with(ApiKeyService::update, req, token).await
}

async fn regenerate(
    test: &Tester,
    token: &str,
    key_id: &str,
) -> Result<api::RegenerateApiKeyResponse, tonic::Status> {
    let req = api::RegenerateApiKeyRequest { id: key_id.into() };
    test.send_with(ApiKeyService::regenerate, req, token).await
}

async fn delete(
    test: &Tester,
    token: &str,
    key_id: &str,
) -> Result<api::DeleteApiKeyResponse, tonic::Status> {
    let req = api::DeleteApiKeyRequest { id: key_id.into() };
    test.send_with(ApiKeyService::delete, req, token).await
}

async fn new_user_token(test: &mut Tester) -> (UserId, String) {
    let (user_id, jwt) = new_user(test).await;
    let token = new_token(test, &jwt, ApiResource::User, user_id).await;
    (user_id, token)
}

async fn new_token<U: Into<Uuid>>(
    test: &mut Tester,
    jwt: &str,
    resource: ApiResource,
    resource_id: U,
) -> String {
    let label = &test.rand_string(8);

    create_api_key(test, jwt, label, resource, resource_id)
        .await
        .unwrap()
        .api_key
        .unwrap()
}

async fn new_user(test: &mut Tester) -> (UserId, jwt::Encoded) {
    let email = test.rand_email();
    create_user(test, &email, OrgRole::Member).await
}

async fn create_user(test: &mut Tester, email: &str, org_role: OrgRole) -> (UserId, jwt::Encoded) {
    let conn = &mut test.conn().await;

    let user = NewUser::new(email, "Test", email, TEST_PASSWORD).unwrap();
    let org_id = seed::ORG_ID.parse().unwrap();
    let created = user.create(conn).await.unwrap();
    let user_id = created.id;

    NewOrgUser::new(org_id, user_id, org_role)
        .create(conn)
        .await
        .unwrap();

    User::confirm(user_id, conn).await.unwrap();

    let claims = login(test, email).await;
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    (user_id, jwt)
}

async fn login(test: &Tester, email: &str) -> Claims {
    let req = api::AuthServiceLoginRequest {
        email: email.to_string(),
        password: TEST_PASSWORD.into(),
    };

    let logged_in = test.send(AuthService::login, req).await.unwrap();
    let token = logged_in.token.into();

    test.cipher().jwt.decode(&token).unwrap()
}
