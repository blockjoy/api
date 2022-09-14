#[allow(dead_code)]
mod setup;

use crate::setup::get_admin_user;
use api::auth::TokenIdentifyable;
use api::grpc::blockjoy_ui::organization_service_client::OrganizationServiceClient;
use api::grpc::blockjoy_ui::{
    CreateOrganizationRequest, DeleteOrganizationRequest, GetOrganizationsRequest, Organization,
    OrganizationMemberRequest, Pagination, RequestMeta, UpdateOrganizationRequest,
    Uuid as GrpcUuid,
};
use api::models::Org;
use setup::{server_and_client_stub, setup};
use std::env;
use std::sync::Arc;
use test_macros::*;
use tonic::transport::Channel;
use tonic::{Request, Status};
use uuid::Uuid;

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_create() {
    let db = Arc::new(_before_values.await);
    let user = get_admin_user(&db).await;
    let token = user.get_token(&db).await.unwrap();
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let org = Organization {
        name: Some("new-org".to_string()),
        id: None,
        created_at: None,
        updated_at: None,
        member_count: None,
        personal: None,
    };
    let inner = CreateOrganizationRequest {
        meta: Some(request_meta),
        organization: Some(org),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { create, request, tonic::Code::Ok, db, OrganizationServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_get() {
    let db = Arc::new(_before_values.await);
    let user = get_admin_user(&db).await;
    let token = user.get_token(&db).await.unwrap();
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let inner = GetOrganizationsRequest {
        meta: Some(request_meta),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { get, request, tonic::Code::Ok, db, OrganizationServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_update() {
    let db = Arc::new(_before_values.await);
    let user = get_admin_user(&db).await;
    let org_id = GrpcUuid::from(
        Org::find_all_by_user(user.id, &db)
            .await
            .unwrap()
            .first()
            .unwrap()
            .id,
    );
    let token = user.get_token(&db).await.unwrap();
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let org = Organization {
        name: Some("new-org-asdf".to_string()),
        id: Some(org_id),
        created_at: None,
        updated_at: None,
        member_count: None,
        personal: None,
    };
    let inner = UpdateOrganizationRequest {
        meta: Some(request_meta),
        organization: Some(org),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { update, request, tonic::Code::Ok, db, OrganizationServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_delete() {
    let db = Arc::new(_before_values.await);
    let user = get_admin_user(&db).await;
    let org_id = GrpcUuid::from(
        Org::find_all_by_user(user.id, &db)
            .await
            .unwrap()
            .first()
            .unwrap()
            .id,
    );
    let token = user.get_token(&db).await.unwrap();
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let inner = DeleteOrganizationRequest {
        meta: Some(request_meta),
        id: Some(org_id),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { delete, request, tonic::Code::Ok, db, OrganizationServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_for_members() {
    let db = Arc::new(_before_values.await);
    let user = get_admin_user(&db).await;
    let org_id = GrpcUuid::from(
        Org::find_all_by_user(user.id, &db)
            .await
            .unwrap()
            .first()
            .unwrap()
            .id,
    );
    let token = user.get_token(&db).await.unwrap();
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let inner = OrganizationMemberRequest {
        meta: Some(request_meta),
        id: Some(org_id),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    assert_grpc_request! { members, request, tonic::Code::Ok, db, OrganizationServiceClient<Channel> };
}

#[before(call = "setup")]
#[tokio::test]
async fn responds_ok_with_pagination_for_members() {
    let db = Arc::new(_before_values.await);
    let pagination = Pagination {
        current_page: 0,
        items_per_page: 10,
        total_items: None,
    };
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: Some(pagination),
    };
    let user = get_admin_user(&db.clone()).await;
    let orgs = Org::find_all_by_user(user.id, &db).await.unwrap();
    let org = orgs.first().unwrap();
    let org_id = GrpcUuid::from(org.id);
    let token = user.get_token(&db).await.unwrap();
    let inner = OrganizationMemberRequest {
        meta: Some(request_meta),
        id: Some(org_id),
    };
    let mut request = Request::new(inner);
    let max_items = env::var("PAGINATION_MAX_ITEMS")
        .unwrap()
        .parse::<i32>()
        .expect("MAX ITEMS NOT SET");

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", token.to_base64()).parse().unwrap(),
    );

    let (serve_future, mut client) =
        server_and_client_stub::<OrganizationServiceClient<Channel>>(db).await;

    let request_future = async {
        match client.members(request).await {
            Ok(response) => {
                let inner = response.into_inner();
                let meta = inner.meta.unwrap();

                assert!(meta.pagination.is_some());

                let pagination = meta.pagination.unwrap();

                assert_eq!(pagination.items_per_page, max_items);
                assert_eq!(pagination.current_page, 0);
                assert_eq!(pagination.total_items.unwrap(), 0);
            }
            Err(e) => {
                panic!("got error: {:?}", e);
            }
        }
    };

    // Wait for completion, when the client request future completes
    tokio::select! {
        _ = serve_future => panic!("server returned first"),
        _ = request_future => (),
    }
}
