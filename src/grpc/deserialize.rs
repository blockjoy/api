//! Deserialize models from gRPC requests

use crate::grpc::blockjoy::UpdateNodeInfoRequest;
use crate::models::Node;

#[tonic::async_trait]
pub trait FromGRPCRequest<R, T> {
    async fn from_grpc_request(_req: R) -> T;
}

#[tonic::async_trait]
impl FromGRPCRequest<UpdateNodeInfoRequest, Node> for Node {
    async fn from_grpc_request(_response: UpdateNodeInfoRequest) -> Node {
        todo!()
    }
}
