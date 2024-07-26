#![allow(dead_code)]

use tonic::transport::Channel;

use blockvisor_api::auth::claims::Claims;
use blockvisor_api::auth::resource::{OrgId, ResourceId, ResourceType, UserId};
use blockvisor_api::auth::token::jwt::Jwt;
use blockvisor_api::database::seed;
use blockvisor_api::grpc::{api, common};
use blockvisor_api::model::org::NewOrg;
use blockvisor_api::model::user::{NewUser, User};

use crate::setup::helper::traits::SocketRpc;
use crate::setup::TestServer;

type ApiKeyService = api::api_key_service_client::ApiKeyServiceClient<Channel>;
type AuthService = api::auth_service_client::AuthServiceClient<Channel>;
type OrgService = api::org_service_client::OrgServiceClient<Channel>;

pub async fn new_seed_user(test: &mut TestServer) -> SeedUser {
    let email = test.rand_email().await;
    let conn = &mut test.conn().await;

    let user = NewUser::new(&email, "Test", &email, seed::LOGIN_PASSWORD).unwrap();
    let created = user.create(conn).await.unwrap();
    let user_id = created.id;
    User::confirm(user_id, conn).await.unwrap();

    let claims = login(test, &email).await;
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    SeedUser {
        user_id,
        email,
        jwt,
    }
}

pub struct SeedUser {
    pub user_id: UserId,
    pub email: String,
    pub jwt: Jwt,
}

pub async fn new_org_user(test: &mut TestServer) -> OrgUser {
    let org_name = test.rand_string(10).await;
    let email = test.rand_email().await;
    let conn = &mut test.conn().await;

    let user = NewUser::new(&email, "Test", &email, seed::LOGIN_PASSWORD).unwrap();
    let created = user.create(conn).await.unwrap();
    let user_id = created.id;
    User::confirm(user_id, conn).await.unwrap();

    let new_org = NewOrg {
        name: &org_name,
        is_personal: false,
    };
    let org = new_org.create(user_id, conn).await.unwrap();
    let org_id = org.id;

    let claims = login(test, &email).await;
    let jwt = test.cipher().jwt.encode(&claims).unwrap();

    OrgUser {
        org_id,
        user_id,
        jwt,
    }
}

pub struct OrgUser {
    pub org_id: OrgId,
    pub user_id: UserId,
    pub jwt: Jwt,
}

pub async fn login(test: &TestServer, email: &str) -> Claims {
    let req = api::AuthServiceLoginRequest {
        email: email.to_string(),
        password: seed::LOGIN_PASSWORD.into(),
    };

    let logged_in = test.send(AuthService::login, req).await.unwrap();
    let token = logged_in.token.into();

    test.cipher().jwt.decode(&token).unwrap()
}

pub async fn new_seed_api_key(test: &mut TestServer) -> SeedApiKey {
    let user = new_seed_user(test).await;
    let user_id = user.user_id;
    let token = new_api_key(test, &user.jwt, ResourceType::User, user_id).await;
    SeedApiKey { user_id, token }
}

pub struct SeedApiKey {
    pub user_id: UserId,
    pub token: String,
}

pub async fn new_api_key<R: Into<ResourceId>>(
    test: &mut TestServer,
    jwt: &Jwt,
    resource: ResourceType,
    resource_id: R,
) -> String {
    let label = &test.rand_string(8).await;

    create_api_key(test, jwt, label, resource, resource_id)
        .await
        .unwrap()
        .api_key
        .unwrap()
}

pub async fn create_api_key<R: Into<ResourceId>>(
    test: &TestServer,
    token: &str,
    label: &str,
    resource: ResourceType,
    resource_id: R,
) -> Result<api::ApiKeyServiceCreateResponse, tonic::Status> {
    let scope = api::ApiKeyScope {
        resource: common::Resource::from(resource).into(),
        resource_id: Some(resource_id.into().to_string()),
    };

    let req = api::ApiKeyServiceCreateRequest {
        label: label.to_string(),
        scope: Some(scope),
    };

    test.send_with(ApiKeyService::create, req, token).await
}
