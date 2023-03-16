use crate::auth::key_provider::KeyProvider;
use crate::errors::ApiError;
use crate::grpc::blockjoy::discovery_server::Discovery;
use crate::grpc::blockjoy::ServicesResponse;
use anyhow::anyhow;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl Discovery for super::GrpcImpl {
    async fn services(&self, _request: Request<()>) -> Result<Response<ServicesResponse>, Status> {
        let response = ServicesResponse {
            key_service_url: std::env::var("KEY_SERVICE_URL").map_err(|e| {
                ApiError::UnexpectedError(anyhow!("Couldn't find key service url: {e}"))
            })?,
            registry_url: std::env::var("COOKBOOK_URL").map_err(|e| {
                ApiError::UnexpectedError(anyhow!("Couldn't find cookbook url: {e}"))
            })?,
            notification_url: format!(
                "{}:{}",
                KeyProvider::get_var("MQTT_SERVER_ADDRESS")
                    .map_err(ApiError::from)?
                    .value,
                KeyProvider::get_var("MQTT_SERVER_PORT")
                    .map_err(ApiError::from)?
                    .value
            ),
        };

        Ok(Response::new(response))
    }
}
