use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::DiscoveryPerm;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction};

use super::api::discovery_service_server::DiscoveryService;
use super::{api, Grpc, Metadata, Status};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) => Status::internal("Internal error."),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl DiscoveryService for Grpc {
    async fn services(
        &self,
        req: Request<api::DiscoveryServiceServicesRequest>,
    ) -> Result<Response<api::DiscoveryServiceServicesResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| services(req, meta.into(), read).scope_boxed())
            .await
    }
}

pub async fn services(
    _: api::DiscoveryServiceServicesRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::DiscoveryServiceServicesResponse, Error> {
    read.auth_all(&meta, DiscoveryPerm::Services).await?;

    Ok(api::DiscoveryServiceServicesResponse {
        notification_url: read.ctx.config.mqtt.notification_url(),
    })
}
