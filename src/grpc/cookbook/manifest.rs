use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::ManifestPerm;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction};
use crate::grpc::api::manifest_service_server::ManifestService;
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
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!("{err}");
        use Error::*;
        match err {
            Cookbook(_) | Diesel(_) => Status::internal("Internal error."),
            MissingId => Status::invalid_argument("id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl ManifestService for Grpc {
    /// Retrieve image for specific version and state.
    async fn retrieve_download_manifest(
        &self,
        req: Request<api::ManifestServiceRetrieveDownloadManifestRequest>,
    ) -> Result<Response<api::ManifestServiceRetrieveDownloadManifestResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| retrieve_download_manifest(req, meta, read).scope_boxed())
            .await
    }
}

async fn retrieve_download_manifest(
    req: api::ManifestServiceRetrieveDownloadManifestRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ManifestServiceRetrieveDownloadManifestResponse, Error> {
    let _ = read.auth_all(&meta, ManifestPerm::RetrieveDownload).await?;

    let id = req.id.ok_or(Error::MissingId)?.into();
    let manifest = read
        .ctx
        .cookbook
        .get_download_manifest(&id, &req.network)
        .await?;

    Ok(api::ManifestServiceRetrieveDownloadManifestResponse {
        manifest: Some(manifest),
    })
}
