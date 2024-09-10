use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::CryptPerm;
use crate::auth::resource::Resource;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::grpc::api::crypt_service_server::CryptService;
use crate::grpc::{api, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing the resource.
    MissingResource,
    /// Claims resource failed: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// Vault error: {0}
    Vault(#[from] crate::store::vault::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) => Status::internal("Internal error."),
            MissingResource => Status::invalid_argument("resource"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Resource(err) => err.into(),
            Vault(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl CryptService for Grpc {
    async fn get_secret(
        &self,
        req: Request<api::CryptServiceGetSecretRequest>,
    ) -> Result<Response<api::CryptServiceGetSecretResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_secret(req, meta, read).scope_boxed())
            .await
    }

    async fn put_secret(
        &self,
        req: Request<api::CryptServicePutSecretRequest>,
    ) -> Result<Response<api::CryptServicePutSecretResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| put_secret(req, meta, write).scope_boxed())
            .await
    }
}

async fn get_secret(
    req: api::CryptServiceGetSecretRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CryptServiceGetSecretResponse, Error> {
    let resource = req.resource.ok_or(Error::MissingResource)?;
    let resource = Resource::try_from(&resource)?;
    let _authz = read.auth_for(&meta, CryptPerm::GetSecret, resource).await?;

    let path = format!("{resource}/secret/{}", req.name);
    let data = read.ctx.vault.read().await.get_bytes(&path).await?;

    Ok(api::CryptServiceGetSecretResponse { value: data })
}

async fn put_secret(
    req: api::CryptServicePutSecretRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::CryptServicePutSecretResponse, Error> {
    let resource = req.resource.ok_or(Error::MissingResource)?;
    let resource = Resource::try_from(&resource)?;
    let _authz = write
        .auth_for(&meta, CryptPerm::PutSecret, resource)
        .await?;

    let path = format!("{resource}/secret/{}", req.name);
    let _version = write
        .ctx
        .vault
        .read()
        .await
        .set_bytes(&path, &req.value)
        .await?;

    Ok(api::CryptServicePutSecretResponse {})
}
