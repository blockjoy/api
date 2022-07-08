//! Serialize models into gRPC response

use crate::grpc::blockjoy::UpdateNodeInfoResponse;
use crate::models::Node;

#[tonic::async_trait]
pub trait IntoGRPCResponse<R, T> {
    async fn into_grpc_response(_response: R) -> T;
}

#[tonic::async_trait]
impl IntoGRPCResponse<UpdateNodeInfoResponse, Node> for Node {
    async fn into_grpc_response(_response: UpdateNodeInfoResponse) -> Node {
        todo!()
    }
}
