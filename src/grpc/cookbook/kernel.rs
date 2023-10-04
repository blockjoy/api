use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::CookbookPerm;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction};
use crate::grpc::api::kernel_service_server::KernelService;
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
        use Error::*;
        error!("{err}");
        match err {
            Cookbook(_) | Diesel(_) => Status::internal("Internal error."),
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
    ) -> Result<Response<api::KernelServiceRetrieveResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| retrieve_kernel_(req, meta, read).scope_boxed())
            .await
    }

    async fn list_kernel_versions(
        &self,
        req: Request<api::KernelServiceListKernelVersionsRequest>,
    ) -> Result<Response<api::KernelServiceListKernelVersionsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_kernel_versions(req, meta, read).scope_boxed())
            .await
    }
}

async fn retrieve_kernel_(
    req: api::KernelServiceRetrieveRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::KernelServiceRetrieveResponse, Error> {
    read.auth_all(&meta, CookbookPerm::RetrieveKernel).await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let url = read.ctx.cookbook.download_kernel(&id.version).await?;

    Ok(api::KernelServiceRetrieveResponse {
        location: Some(api::ArchiveLocation { url }),
    })
}

async fn list_kernel_versions(
    _req: api::KernelServiceListKernelVersionsRequest,
    _meta: MetadataMap,
    read: ReadConn<'_, '_>,
) -> Result<api::KernelServiceListKernelVersionsResponse, Error> {
    let identifiers = read.ctx.cookbook.list_kernels().await?;

    Ok(api::KernelServiceListKernelVersionsResponse { identifiers })
}
