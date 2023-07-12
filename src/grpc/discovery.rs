use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::endpoint::Endpoint;
use crate::config::Context;
use crate::database::{Conn, Transaction};

use super::api::{self, discovery_service_server};

#[tonic::async_trait]
impl discovery_service_server::DiscoveryService for super::Grpc {
    async fn services(
        &self,
        req: tonic::Request<api::DiscoveryServiceServicesRequest>,
    ) -> super::Resp<api::DiscoveryServiceServicesResponse> {
        self.context
            .read(|conn, ctx| services(req, conn, ctx).scope_boxed())
            .await
    }
}

async fn services(
    req: tonic::Request<api::DiscoveryServiceServicesRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::DiscoveryServiceServicesResponse> {
    let _claims = ctx.claims(&req, Endpoint::DiscoveryServices).await?;

    let response = api::DiscoveryServiceServicesResponse {
        key_service_url: ctx.config.key_service.url.to_string(),
        notification_url: ctx.config.mqtt.notification_url(),
    };

    Ok(tonic::Response::new(response))
}
