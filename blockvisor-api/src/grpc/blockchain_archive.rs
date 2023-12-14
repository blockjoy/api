use std::time::Duration;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::BlockchainArchivePerm;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction};
use crate::grpc::api::blockchain_archive_service_server::BlockchainArchiveService;
use crate::grpc::{api, Grpc};
use crate::storage::image::ImageId;

const DEFAULT_EXPIRES: u32 = 60 * 60 * 24;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Storage failed: {0}
    Storage(#[from] crate::storage::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing DownloadManifest.
    MissingDownloadManifest,
    /// Missing blockchain id.
    MissingId,
    /// Failed to parse ImageId: {0}
    ParseImageId(crate::storage::image::Error),
    /// Failed to parse DownloadManifest: {0}
    ParseDownloadManifest(crate::storage::manifest::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | Storage(_) => Status::internal("Internal error."),
            MissingDownloadManifest | ParseDownloadManifest(_) => {
                Status::invalid_argument("manifest")
            }
            MissingId | ParseImageId(_) => Status::invalid_argument("id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl BlockchainArchiveService for Grpc {
    async fn get_download_manifest(
        &self,
        req: Request<api::BlockchainArchiveServiceGetDownloadManifestRequest>,
    ) -> Result<Response<api::BlockchainArchiveServiceGetDownloadManifestResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_download_manifest(req, meta, read).scope_boxed())
            .await
    }

    async fn get_upload_manifest(
        &self,
        req: Request<api::BlockchainArchiveServiceGetUploadManifestRequest>,
    ) -> Result<Response<api::BlockchainArchiveServiceGetUploadManifestResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_upload_manifest(req, meta, read).scope_boxed())
            .await
    }

    async fn put_download_manifest(
        &self,
        req: Request<api::BlockchainArchiveServicePutDownloadManifestRequest>,
    ) -> Result<Response<api::BlockchainArchiveServicePutDownloadManifestResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| put_download_manifest(req, meta, read).scope_boxed())
            .await
    }
}

async fn get_download_manifest(
    req: api::BlockchainArchiveServiceGetDownloadManifestRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainArchiveServiceGetDownloadManifestResponse, Error> {
    read.auth_all(&meta, BlockchainArchivePerm::GetDownload)
        .await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let image = ImageId::try_from(id).map_err(Error::ParseImageId)?;
    let manifest = read
        .ctx
        .storage
        .download_manifest(&image, &req.network)
        .await?;

    Ok(api::BlockchainArchiveServiceGetDownloadManifestResponse {
        manifest: Some(manifest.into()),
    })
}

async fn get_upload_manifest(
    req: api::BlockchainArchiveServiceGetUploadManifestRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainArchiveServiceGetUploadManifestResponse, Error> {
    read.auth_all(&meta, BlockchainArchivePerm::GetUpload)
        .await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let image = ImageId::try_from(id).map_err(Error::ParseImageId)?;
    let expires = Duration::from_secs(req.url_expires.unwrap_or(DEFAULT_EXPIRES).into());

    let manifest = read
        .ctx
        .storage
        .upload_manifest(&image, &req.network, req.data_version, req.slots, expires)
        .await?;

    Ok(api::BlockchainArchiveServiceGetUploadManifestResponse {
        manifest: Some(manifest.into()),
    })
}

async fn put_download_manifest(
    req: api::BlockchainArchiveServicePutDownloadManifestRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainArchiveServicePutDownloadManifestResponse, Error> {
    read.auth_all(&meta, BlockchainArchivePerm::PutDownload)
        .await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let image = ImageId::try_from(id).map_err(Error::ParseImageId)?;
    let manifest = req.manifest.ok_or(Error::MissingDownloadManifest)?;
    let manifest = manifest.try_into().map_err(Error::ParseDownloadManifest)?;

    read.ctx
        .storage
        .save_download_manifest(&image, &req.network, &manifest)
        .await?;

    Ok(api::BlockchainArchiveServicePutDownloadManifestResponse {})
}
