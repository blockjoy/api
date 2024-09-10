use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::CryptPerm;
use crate::auth::resource::{ResourceEntry, ResourceId, ResourceType};
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
    /// Failed to parse Resource: {0}
    ParseResource(crate::auth::resource::Error),
    /// Failed to parse ResourceId: {0}
    ParseResourceId(uuid::Error),
    /// Vault error: {0}
    Vault(#[from] crate::storage::vault::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) => Status::internal("Internal error."),
            ParseResource(_) => Status::invalid_argument("resource"),
            ParseResourceId(_) => Status::invalid_argument("resource_id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
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
    let resource: ResourceType = req.resource().try_into().map_err(Error::ParseResource)?;
    let resource_id: ResourceId = req.resource_id.parse().map_err(Error::ParseResourceId)?;
    let entry = ResourceEntry::new(resource, resource_id);
    let _authz = read.auth(&meta, CryptPerm::GetSecret, entry).await?;

    let path = format!("{resource}/{resource_id}/secret/{}", req.name);
    let data = read.ctx.vault.read().await.get_bytes(&path).await?;

    Ok(api::CryptServiceGetSecretResponse { value: data })
}

async fn put_secret(
    req: api::CryptServicePutSecretRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::CryptServicePutSecretResponse, Error> {
    let resource: ResourceType = req.resource().try_into().map_err(Error::ParseResource)?;
    let resource_id: ResourceId = req.resource_id.parse().map_err(Error::ParseResourceId)?;
    let entry = ResourceEntry::new(resource, resource_id);
    let _authz = write.auth(&meta, CryptPerm::PutSecret, entry).await?;

    let path = format!("{resource}/{resource_id}/secret/{}", req.name);
    let _version = write
        .ctx
        .vault
        .read()
        .await
        .set_bytes(&path, &req.value)
        .await?;

    Ok(api::CryptServicePutSecretResponse {})
}
