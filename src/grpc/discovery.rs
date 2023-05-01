use super::api::{self, discovery_service_server};
use crate::auth::key_provider::KeyProvider;
use crate::Error;
use anyhow::anyhow;
use tonic::{Request, Response};

#[tonic::async_trait]
impl discovery_service_server::DiscoveryService for super::GrpcImpl {
    async fn services(
        &self,
        _request: Request<api::DiscoveryServiceServicesRequest>,
    ) -> super::Result<api::DiscoveryServiceServicesResponse> {
        let response = api::DiscoveryServiceServicesResponse {
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
