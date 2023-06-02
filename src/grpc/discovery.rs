use super::api::{self, discovery_service_server};
use crate::{
    auth::{self, key_provider::KeyProvider},
    models,
};

#[tonic::async_trait]
impl discovery_service_server::DiscoveryService for super::GrpcImpl {
    async fn services(
        &self,
        req: tonic::Request<api::DiscoveryServiceServicesRequest>,
    ) -> super::Resp<api::DiscoveryServiceServicesResponse> {
        let mut conn = self.conn().await?;
        let resp = services(req, &mut conn).await?;
        Ok(resp)
    }
}

async fn services(
    req: tonic::Request<api::DiscoveryServiceServicesRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::DiscoveryServiceServicesResponse> {
    auth::get_claims(&req, auth::Endpoint::DiscoveryServices, conn).await?;
    let mqtt_address = KeyProvider::get_var("MQTT_SERVER_ADDRESS")?;
    let mqtt_port = KeyProvider::get_var("MQTT_SERVER_PORT")?;
    let mqtt_scheme = match mqtt_port.as_str() {
        "8083" => "ws",
        "8084" => "wss",
        "8883" => "mqtts",
        _ => "mqtt",
    };
    let err = |name| move |e| crate::Error::unexpected(format!("Couldn't find {name}: {e}"));
    let response = api::DiscoveryServiceServicesResponse {
        key_service_url: std::env::var("KEY_SERVICE_URL").map_err(err("key service url"))?,
        registry_url: KeyProvider::get_var(cookbook::COOKBOOK_URL)?,
        notification_url: format!("{mqtt_scheme}://{mqtt_address}:{mqtt_port}"),
    };
    Ok(tonic::Response::new(response))
}
