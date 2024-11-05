use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::KernelPerm;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction};
use crate::grpc::api::kernel_service_server::KernelService;
use crate::grpc::{api, common, Grpc};

use super::{Metadata, Status};

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
    /// Storage failed: {0}
    Storage(#[from] crate::storage::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | Storage(_) => Status::internal("Internal error."),
            MissingId => Status::invalid_argument("id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl KernelService for Grpc {
    async fn retrieve(
        &self,
        req: Request<api::KernelServiceRetrieveRequest>,
    ) -> Result<Response<api::KernelServiceRetrieveResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| retrieve_kernel(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn list_kernel_versions(
        &self,
        req: Request<api::KernelServiceListKernelVersionsRequest>,
    ) -> Result<Response<api::KernelServiceListKernelVersionsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_kernel_versions(req, meta.into(), read).scope_boxed())
            .await
    }
}

pub async fn retrieve_kernel(
    req: api::KernelServiceRetrieveRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::KernelServiceRetrieveResponse, Error> {
    read.auth_all(&meta, KernelPerm::Retrieve).await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let url = read.ctx.storage.download_kernel(&id.version).await?;

    Ok(api::KernelServiceRetrieveResponse {
        location: Some(common::ArchiveLocation {
            url: url.to_string(),
        }),
    })
}

pub async fn list_kernel_versions(
    _: api::KernelServiceListKernelVersionsRequest,
    _: Metadata,
    read: ReadConn<'_, '_>,
) -> Result<api::KernelServiceListKernelVersionsResponse, Error> {
    let identifiers = read.ctx.storage.list_kernels().await?;

    Ok(api::KernelServiceListKernelVersionsResponse { identifiers })
}
