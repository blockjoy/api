#[allow(dead_code)]
mod setup;

use crate::setup::{server_and_client_stub, setup};
use api::grpc::blockjoy_ui::update_service_client::UpdateServiceClient;
use api::grpc::blockjoy_ui::{GetUpdatesRequest, RequestMeta, Uuid as GrpcUuid};
use std::sync::Arc;
use test_macros::before;
use tonic::{transport::Channel, Request, Status};
use uuid::Uuid;

#[before(call = "setup")]
#[tokio::test]
async fn responds_unauthenticated_with_invalid_token_for_update() {
    let db = Arc::new(_before_values.await);
    let request_meta = RequestMeta {
        id: Some(GrpcUuid::from(Uuid::new_v4())),
        token: None,
        fields: vec![],
        pagination: None,
    };
    let inner = GetUpdatesRequest {
        meta: Some(request_meta),
    };
    let mut request = Request::new(inner);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", "some-invalid-token").parse().unwrap(),
    );

    assert_grpc_request! { updates, request, tonic::Code::Unauthenticated, db, UpdateServiceClient<Channel> };
}
