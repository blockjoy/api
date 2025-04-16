use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::Authorize;
use crate::auth::rbac::DiscoveryPerm;
use crate::database::{ReadConn, Transaction};

use super::api::discovery_service_server::DiscoveryService;
use super::{Grpc, Metadata, Status, api};

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
    async fn api_version(
        &self,
        req: Request<api::DiscoveryServiceApiVersionRequest>,
    ) -> Result<Response<api::DiscoveryServiceApiVersionResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| api_version(req, meta.into(), read).scope_boxed())
            .await
    }
}

pub async fn services(
    _: api::DiscoveryServiceServicesRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::DiscoveryServiceServicesResponse, Error> {
    read.auth(&meta, DiscoveryPerm::Services).await?;

    Ok(api::DiscoveryServiceServicesResponse {
        notification_url: read.ctx.config.mqtt.notification_url(),
    })
}

pub async fn api_version(
    _: api::DiscoveryServiceApiVersionRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::DiscoveryServiceApiVersionResponse, Error> {
    read.auth(&meta, DiscoveryPerm::ApiVersion).await?;

    Ok(api::DiscoveryServiceApiVersionResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
