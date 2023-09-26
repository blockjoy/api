use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::BundlePerm;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction};
use crate::grpc::api::bundle_service_server::BundleService;
use crate::grpc::{api, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Cookbook failed: {0}
    Cookbook(#[from] crate::cookbook::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing cookbook id.
    MissingId,
    /// This endpoint is not currently used.
    NotUsed,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!("{err}");
        use Error::*;
        match err {
            Cookbook(_) | Diesel(_) | NotUsed => Status::internal("Internal error."),
            MissingId => Status::invalid_argument("id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl BundleService for Grpc {
    async fn retrieve(
        &self,
        req: Request<api::BundleServiceRetrieveRequest>,
    ) -> Result<Response<api::BundleServiceRetrieveResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| retrieve(req, meta, read).scope_boxed())
            .await
    }

    async fn list_bundle_versions(
        &self,
        req: Request<api::BundleServiceListBundleVersionsRequest>,
    ) -> Result<Response<api::BundleServiceListBundleVersionsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_bundle_versions(req, meta, read).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::BundleServiceDeleteRequest>,
    ) -> Result<Response<api::BundleServiceDeleteResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| delete(req, meta, read).scope_boxed())
            .await
    }
}

/// Retrieve image for specific version and state.
async fn retrieve(
    req: api::BundleServiceRetrieveRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BundleServiceRetrieveResponse, Error> {
    let _ = read.auth_all(&meta, BundlePerm::Retrieve).await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let url = read.ctx.cookbook.bundle_download_url(&id.version).await?;

    Ok(api::BundleServiceRetrieveResponse {
        location: Some(api::ArchiveLocation { url }),
    })
}

/// List all available bundle versions.
async fn list_bundle_versions(
    _req: api::BundleServiceListBundleVersionsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BundleServiceListBundleVersionsResponse, Error> {
    let _ = read.auth_all(&meta, BundlePerm::ListBundleVersions).await?;
    let identifiers = read.ctx.cookbook.list_bundles().await?;

    Ok(api::BundleServiceListBundleVersionsResponse { identifiers })
}

/// Delete bundle from storage.
async fn delete(
    _req: api::BundleServiceDeleteRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BundleServiceDeleteResponse, Error> {
    let _ = read.auth_all(&meta, BundlePerm::Delete).await?;

    Err(Error::NotUsed)
}
