use super::api::{self, babel_service_server};

#[tonic::async_trait]
impl babel_service_server::BabelService for super::Grpc {
    async fn notify(
        &self,
        _req: tonic::Request<api::BabelServiceNotifyRequest>,
    ) -> super::Resp<api::BabelServiceNotifyResponse> {
        Err(tonic::Status::unimplemented("Node upgrade needs a rework"))
    }
}
