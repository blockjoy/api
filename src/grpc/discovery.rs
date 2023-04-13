use super::api::ServicesResponse;
use super::api::{self, discovery_server};
use crate::auth::key_provider::KeyProvider;
use crate::Error;
use anyhow::anyhow;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl discovery_server::Discovery for super::GrpcImpl {
    async fn services(
        &self,
        _request: Request<api::ServicesRequest>,
    ) -> Result<Response<ServicesResponse>, Status> {
        let response = ServicesResponse {
            key_service_url: std::env::var("KEY_SERVICE_URL").map_err(|e| {
                Error::UnexpectedError(anyhow!("Couldn't find key service url: {e}"))
            })?,
            registry_url: std::env::var("COOKBOOK_URL")
                .map_err(|e| Error::UnexpectedError(anyhow!("Couldn't find cookbook url: {e}")))?,
            notification_url: format!(
                "{}:{}",
                KeyProvider::get_var("MQTT_SERVER_ADDRESS")
                    .map_err(crate::Error::from)?
                    .value,
                KeyProvider::get_var("MQTT_SERVER_PORT")
                    .map_err(crate::Error::from)?
                    .value
            ),
        };

        Ok(Response::new(response))
    }
}
