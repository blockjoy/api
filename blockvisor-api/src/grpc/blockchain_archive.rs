use std::time::Duration;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::BlockchainArchivePerm;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction};
use crate::grpc::api::blockchain_archive_service_server::BlockchainArchiveService;
use crate::grpc::{api, Grpc};
use crate::storage::image::ImageId;
use crate::storage::manifest::DownloadManifest;

use super::{Metadata, Status};

const DEFAULT_EXPIRES: u32 = 7 * 24 * 60 * 60;
const MAX_CHUNK_INDEXES: usize = 100;
const MAX_SLOT_INDEXES: usize = 100;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Failed to parse chunk index: {0}
    ChunkIndex(std::num::TryFromIntError),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing blockchain id.
    MissingId,
    /// Failed to parse ArchiveChunk: {0}
    ParseChunk(crate::storage::manifest::Error),
    /// Failed to parse UploadSlot: {0}
    ParseSlot(crate::storage::manifest::Error),
    /// Failed to parse Compression: {0}
    ParseCompression(crate::storage::manifest::Error),
    /// Failed to parse ImageId: {0}
    ParseImageId(crate::storage::image::Error),
    /// Failed to parse slot index: {0}
    SlotIndex(std::num::TryFromIntError),
    /// Storage failed: {0}
    Storage(#[from] crate::storage::Error),
    /// Too many chunk indexes requested.
    TooManyChunks,
    /// Too many slot indexes requested.
    TooManySlots,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) => Status::internal("Internal error."),
            MissingId | ParseImageId(_) => Status::invalid_argument("id"),
            ParseChunk(_) => Status::invalid_argument("chunks"),
            ParseSlot(_) => Status::invalid_argument("slots"),
            ParseCompression(_) => Status::invalid_argument("compression"),
            ChunkIndex(_) | TooManyChunks => Status::out_of_range("chunk_indexes"),
            SlotIndex(_) | TooManySlots => Status::out_of_range("slot_indexes"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Storage(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl BlockchainArchiveService for Grpc {
    async fn get_download_metadata(
        &self,
        req: Request<api::BlockchainArchiveServiceGetDownloadMetadataRequest>,
    ) -> Result<Response<api::BlockchainArchiveServiceGetDownloadMetadataResponse>, tonic::Status>
    {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_download_metadata(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn get_download_chunks(
        &self,
        req: Request<api::BlockchainArchiveServiceGetDownloadChunksRequest>,
    ) -> Result<Response<api::BlockchainArchiveServiceGetDownloadChunksResponse>, tonic::Status>
    {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_download_chunks(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn get_upload_slots(
        &self,
        req: Request<api::BlockchainArchiveServiceGetUploadSlotsRequest>,
    ) -> Result<Response<api::BlockchainArchiveServiceGetUploadSlotsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_upload_slots(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn put_download_manifest(
        &self,
        req: Request<api::BlockchainArchiveServicePutDownloadManifestRequest>,
    ) -> Result<Response<api::BlockchainArchiveServicePutDownloadManifestResponse>, tonic::Status>
    {
        let (meta, _, req) = req.into_parts();
        self.read(|read| put_download_manifest(req, meta.into(), read).scope_boxed())
            .await
    }
}

pub async fn get_download_metadata(
    req: api::BlockchainArchiveServiceGetDownloadMetadataRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainArchiveServiceGetDownloadMetadataResponse, Error> {
    read.auth_all(&meta, BlockchainArchivePerm::GetDownloadMetadata)
        .await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let image = ImageId::try_from(id).map_err(Error::ParseImageId)?;
    let (header, data_version) = read
        .ctx
        .storage
        .find_download_manifest_header(&image, &req.network, req.data_version)
        .await?;

    Ok(api::BlockchainArchiveServiceGetDownloadMetadataResponse {
        data_version,
        total_size: header.total_size,
        compression: header.compression.map(Into::into),
        chunks: header.chunks,
    })
}

pub async fn get_download_chunks(
    req: api::BlockchainArchiveServiceGetDownloadChunksRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainArchiveServiceGetDownloadChunksResponse, Error> {
    read.auth_all(&meta, BlockchainArchivePerm::GetDownloadChunks)
        .await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let image = ImageId::try_from(id).map_err(Error::ParseImageId)?;

    if req.chunk_indexes.len() > MAX_CHUNK_INDEXES {
        return Err(Error::TooManyChunks);
    }

    let indexes = req
        .chunk_indexes
        .iter()
        .map(|i| usize::try_from(*i).map_err(Error::ChunkIndex))
        .collect::<Result<Vec<_>, _>>()?;
    let chunks = read
        .ctx
        .storage
        .refresh_download_manifest(&image, &req.network, req.data_version, &indexes)
        .await?;

    Ok(api::BlockchainArchiveServiceGetDownloadChunksResponse {
        chunks: chunks
            .into_iter()
            .map(|chunk| chunk.try_into().map_err(Error::ParseChunk))
            .collect::<Result<_, _>>()?,
    })
}

pub async fn get_upload_slots(
    req: api::BlockchainArchiveServiceGetUploadSlotsRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainArchiveServiceGetUploadSlotsResponse, Error> {
    read.auth_all(&meta, BlockchainArchivePerm::GetUploadSlots)
        .await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let image = ImageId::try_from(id).map_err(Error::ParseImageId)?;
    let expires = Duration::from_secs(req.url_expires.unwrap_or(DEFAULT_EXPIRES).into());

    if req.slot_indexes.len() > MAX_SLOT_INDEXES {
        return Err(Error::TooManySlots);
    }

    let indexes = req
        .slot_indexes
        .iter()
        .map(|i| usize::try_from(*i).map_err(Error::SlotIndex))
        .collect::<Result<Vec<_>, _>>()?;

    let (slots, data_version) = read
        .ctx
        .storage
        .upload_slots(&image, &req.network, req.data_version, &indexes, expires)
        .await?;

    Ok(api::BlockchainArchiveServiceGetUploadSlotsResponse {
        data_version,
        slots: slots
            .into_iter()
            .map(|slot| slot.try_into().map_err(Error::ParseSlot))
            .collect::<Result<_, _>>()?,
    })
}

pub async fn put_download_manifest(
    req: api::BlockchainArchiveServicePutDownloadManifestRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainArchiveServicePutDownloadManifestResponse, Error> {
    read.auth_all(&meta, BlockchainArchivePerm::PutDownloadManifest)
        .await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let image = ImageId::try_from(id).map_err(Error::ParseImageId)?;
    let manifest = DownloadManifest {
        total_size: req.total_size,
        compression: req
            .compression
            .map(TryInto::try_into)
            .transpose()
            .map_err(Error::ParseCompression)?,
        chunks: req
            .chunks
            .into_iter()
            .map(|chunk| chunk.try_into().map_err(Error::ParseChunk))
            .collect::<Result<Vec<_>, _>>()?,
    };

    read.ctx
        .storage
        .save_download_manifest(&image, &req.network, req.data_version, manifest)
        .await?;

    Ok(api::BlockchainArchiveServicePutDownloadManifestResponse {})
}
