use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::BundlePerm;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction};
use crate::grpc::api::bundle_service_server::BundleService;
use crate::grpc::{api, common, Grpc};

use super::Status;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing image identifier.
    MissingId,
    /// This endpoint is not currently used.
    NotUsed,
    /// Storage failed: {0}
    Storage(#[from] crate::storage::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | NotUsed | Storage(_) => Status::internal("Internal error."),
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
    ) -> Result<Response<api::BundleServiceRetrieveResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| retrieve(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn list_bundle_versions(
        &self,
        req: Request<api::BundleServiceListBundleVersionsRequest>,
    ) -> Result<Response<api::BundleServiceListBundleVersionsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_bundle_versions(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::BundleServiceDeleteRequest>,
    ) -> Result<Response<api::BundleServiceDeleteResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| delete(req, meta.into(), read).scope_boxed())
            .await
    }
}

/// Retrieve image for specific version and state.
pub async fn retrieve(
    req: api::BundleServiceRetrieveRequest,
    meta: super::NaiveMeta,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BundleServiceRetrieveResponse, Error> {
    read.auth_all(&meta, BundlePerm::Retrieve).await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let url = read.ctx.storage.download_bundle(&id.version).await?;

    Ok(api::BundleServiceRetrieveResponse {
        location: Some(common::ArchiveLocation {
            url: url.to_string(),
        }),
    })
}

/// List all available bundle versions.
pub async fn list_bundle_versions(
    _: api::BundleServiceListBundleVersionsRequest,
    meta: super::NaiveMeta,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BundleServiceListBundleVersionsResponse, Error> {
    read.auth_all(&meta, BundlePerm::ListBundleVersions).await?;
    let identifiers = read.ctx.storage.list_bundles().await?;

    Ok(api::BundleServiceListBundleVersionsResponse { identifiers })
}

/// Delete bundle from storage.
pub async fn delete(
    _: api::BundleServiceDeleteRequest,
    meta: super::NaiveMeta,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BundleServiceDeleteResponse, Error> {
    read.auth_all(&meta, BundlePerm::Delete).await?;

    Err(Error::NotUsed)
}
