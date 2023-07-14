use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::endpoint::Endpoint;
use crate::models;

use super::api::{self, discovery_service_server};

#[tonic::async_trait]
impl discovery_service_server::DiscoveryService for super::Grpc {
    async fn services(
        &self,
        req: tonic::Request<api::DiscoveryServiceServicesRequest>,
    ) -> super::Resp<api::DiscoveryServiceServicesResponse> {
        self.run(|c| services(req, c).scope_boxed()).await
    }
}

async fn services(
    req: tonic::Request<api::DiscoveryServiceServicesRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::DiscoveryServiceServicesResponse> {
    let _claims = conn.claims(&req, Endpoint::DiscoveryServices).await?;

    let response = api::DiscoveryServiceServicesResponse {
        key_service_url: conn.context.config.key_service.url.to_string(),
        notification_url: conn.context.config.mqtt.notification_url(),
    };

    Ok(tonic::Response::new(response))
}
