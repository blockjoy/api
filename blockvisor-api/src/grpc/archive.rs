use std::time::Duration;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::{ArchiveAdminPerm, ArchivePerm};
use crate::auth::resource::Resources;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction};
use crate::grpc::api::archive_service_server::ArchiveService;
use crate::grpc::{api, Grpc};
use crate::model::image::Archive;
use crate::store::manifest::DownloadManifest;

const DEFAULT_EXPIRES: u32 = 7 * 24 * 60 * 60;
const MAX_CHUNK_INDEXES: usize = 100;
const MAX_SLOT_INDEXES: usize = 100;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Archive image error: {0}
    Archive(#[from] crate::model::image::archive::Error),
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Failed to parse chunk index: {0}
    ChunkIndex(std::num::TryFromIntError),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Failed to parse archive_id: {0}
    ParseArchiveId(uuid::Error),
    /// Failed to parse ArchiveChunk: {0}
    ParseChunk(crate::store::manifest::Error),
    /// Failed to parse Compression: {0}
    ParseCompression(crate::store::manifest::Error),
    /// Failed to parse org_id: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse UploadSlot: {0}
    ParseSlot(crate::store::manifest::Error),
    /// Failed to parse slot index: {0}
    SlotIndex(std::num::TryFromIntError),
    /// Store failed: {0}
    Store(#[from] crate::store::Error),
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
            ParseArchiveId(_) => Status::invalid_argument("archive_id"),
            ParseChunk(_) => Status::invalid_argument("chunks"),
            ParseCompression(_) => Status::invalid_argument("compression"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseSlot(_) => Status::invalid_argument("slots"),
            ChunkIndex(_) | TooManyChunks => Status::out_of_range("chunk_indexes"),
            SlotIndex(_) | TooManySlots => Status::out_of_range("slot_indexes"),
            Archive(err) => err.into(),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Store(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl ArchiveService for Grpc {
    async fn get_download_metadata(
        &self,
        req: Request<api::ArchiveServiceGetDownloadMetadataRequest>,
    ) -> Result<Response<api::ArchiveServiceGetDownloadMetadataResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_download_metadata(req, meta, read).scope_boxed())
            .await
    }

    async fn get_download_chunks(
        &self,
        req: Request<api::ArchiveServiceGetDownloadChunksRequest>,
    ) -> Result<Response<api::ArchiveServiceGetDownloadChunksResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_download_chunks(req, meta, read).scope_boxed())
            .await
    }

    async fn get_upload_slots(
        &self,
        req: Request<api::ArchiveServiceGetUploadSlotsRequest>,
    ) -> Result<Response<api::ArchiveServiceGetUploadSlotsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_upload_slots(req, meta, read).scope_boxed())
            .await
    }

    async fn put_download_manifest(
        &self,
        req: Request<api::ArchiveServicePutDownloadManifestRequest>,
    ) -> Result<Response<api::ArchiveServicePutDownloadManifestResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| put_download_manifest(req, meta, read).scope_boxed())
            .await
    }
}

async fn get_download_metadata(
    req: api::ArchiveServiceGetDownloadMetadataRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ArchiveServiceGetDownloadMetadataResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let _authz = read
        .auth_or_for(
            &meta,
            ArchiveAdminPerm::GetDownloadMetadata,
            ArchivePerm::GetDownloadMetadata,
            resources,
        )
        .await?;

    let archive_id = req.archive_id.parse().map_err(Error::ParseArchiveId)?;
    let archive = Archive::by_id(archive_id, org_id, &mut read).await?;
    let (header, data_version) = read
        .ctx
        .store
        .download_manifest_header(&archive.store_id, req.data_version)
        .await?;

    Ok(api::ArchiveServiceGetDownloadMetadataResponse {
        data_version,
        total_size: header.total_size,
        compression: header.compression.map(Into::into),
        chunks: header.chunks,
    })
}

async fn get_download_chunks(
    req: api::ArchiveServiceGetDownloadChunksRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ArchiveServiceGetDownloadChunksResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let _authz = read
        .auth_or_for(
            &meta,
            ArchiveAdminPerm::GetDownloadChunks,
            ArchivePerm::GetDownloadChunks,
            resources,
        )
        .await?;

    let archive_id = req.archive_id.parse().map_err(Error::ParseArchiveId)?;
    let archive = Archive::by_id(archive_id, org_id, &mut read).await?;

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
        .store
        .refresh_download_manifest(&archive.store_id, req.data_version, &indexes)
        .await?;

    Ok(api::ArchiveServiceGetDownloadChunksResponse {
        chunks: chunks
            .into_iter()
            .map(|chunk| chunk.try_into().map_err(Error::ParseChunk))
            .collect::<Result<_, _>>()?,
    })
}

async fn get_upload_slots(
    req: api::ArchiveServiceGetUploadSlotsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ArchiveServiceGetUploadSlotsResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let _authz = read
        .auth_or_for(
            &meta,
            ArchiveAdminPerm::GetUploadSlots,
            ArchivePerm::GetUploadSlots,
            resources,
        )
        .await?;

    let archive_id = req.archive_id.parse().map_err(Error::ParseArchiveId)?;
    let archive = Archive::by_id(archive_id, org_id, &mut read).await?;
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
        .store
        .upload_slots(&archive.store_id, req.data_version, &indexes, expires)
        .await?;

    Ok(api::ArchiveServiceGetUploadSlotsResponse {
        data_version,
        slots: slots
            .into_iter()
            .map(|slot| slot.try_into().map_err(Error::ParseSlot))
            .collect::<Result<_, _>>()?,
    })
}

async fn put_download_manifest(
    req: api::ArchiveServicePutDownloadManifestRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ArchiveServicePutDownloadManifestResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let _authz = read
        .auth_or_for(
            &meta,
            ArchiveAdminPerm::PutDownloadManifest,
            ArchivePerm::PutDownloadManifest,
            resources,
        )
        .await?;

    let archive_id = req.archive_id.parse().map_err(Error::ParseArchiveId)?;
    let archive = Archive::by_id(archive_id, org_id, &mut read).await?;

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
        .store
        .save_download_manifest(&archive.store_id, manifest)
        .await?;

    Ok(api::ArchiveServicePutDownloadManifestResponse {})
}
