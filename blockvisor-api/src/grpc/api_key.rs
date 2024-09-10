use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::ApiKeyPerm;
use crate::auth::resource::Resource;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::model::api_key::{ApiKey, NewApiKey, UpdateLabel};
use crate::util::NanosUtc;

use super::api::api_key_service_server::ApiKeyService;
use super::{api, common, Grpc, Metadata, Status};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Claims Resource is not a user.
    ClaimsNotUser,
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Request is missing the resource.
    MissingResource,
    /// Missing API key `updated_at`.
    MissingUpdatedAt,
    /// Database model error: {0}
    Model(#[from] crate::model::api_key::Error),
    /// Nothing is set to be updated in the request.
    NothingToUpdate,
    /// Failed to parse KeyId: {0}
    ParseKeyId(crate::auth::token::api_key::Error),
    /// API key resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            ClaimsNotUser => Status::forbidden("Access denied."),
            Diesel(_) | MissingUpdatedAt => Status::internal("Internal error."),
            MissingResource => Status::invalid_argument("resource"),
            NothingToUpdate => Status::failed_precondition("Nothing to update."),
            ParseKeyId(_) => Status::invalid_argument("id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Model(err) => err.into(),
            Resource(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl ApiKeyService for Grpc {
    async fn create(
        &self,
        req: Request<api::ApiKeyServiceCreateRequest>,
    ) -> Result<Response<api::ApiKeyServiceCreateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::ApiKeyServiceListRequest>,
    ) -> Result<Response<api::ApiKeyServiceListResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn update(
        &self,
        req: Request<api::ApiKeyServiceUpdateRequest>,
    ) -> Result<Response<api::ApiKeyServiceUpdateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn regenerate(
        &self,
        req: Request<api::ApiKeyServiceRegenerateRequest>,
    ) -> Result<Response<api::ApiKeyServiceRegenerateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| regenerate(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::ApiKeyServiceDeleteRequest>,
    ) -> Result<Response<api::ApiKeyServiceDeleteResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta.into(), write).scope_boxed())
            .await
    }
}

pub async fn create(
    req: api::ApiKeyServiceCreateRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ApiKeyServiceCreateResponse, Error> {
    let resource = req.resource.ok_or(Error::MissingResource)?;
    let resource = Resource::try_from(&resource)?;
    let authz = write.auth_for(&meta, ApiKeyPerm::Create, resource).await?;

    let user_id = authz.resource().user().ok_or(Error::ClaimsNotUser)?;
    let created = NewApiKey::create(user_id, req.label, resource, &mut write).await?;

    Ok(api::ApiKeyServiceCreateResponse {
        api_key: Some(created.secret.into()),
        created_at: Some(NanosUtc::from(created.api_key.created_at).into()),
    })
}

pub async fn list(
    _: api::ApiKeyServiceListRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ApiKeyServiceListResponse, Error> {
    let authz = read.auth(&meta, ApiKeyPerm::List).await?;
    let user_id = authz.resource().user().ok_or(Error::ClaimsNotUser)?;

    let keys = ApiKey::by_user_id(user_id, &mut read).await?;
    let api_keys = keys.into_iter().map(api::ListApiKey::from).collect();

    Ok(api::ApiKeyServiceListResponse { api_keys })
}

pub async fn update(
    req: api::ApiKeyServiceUpdateRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ApiKeyServiceUpdateResponse, Error> {
    let key_id = req.api_key_id.parse().map_err(Error::ParseKeyId)?;
    let existing = ApiKey::by_id(key_id, &mut write).await?;
    write.auth_for(&meta, ApiKeyPerm::Update, &existing).await?;

    let mut updated_at = None;

    if let Some(label) = req.label {
        updated_at = UpdateLabel::new(key_id, label)
            .update(&mut write)
            .await
            .map(Some)?;
    }

    let updated_at = updated_at
        .ok_or(Error::NothingToUpdate)
        .map(NanosUtc::from)
        .map(Into::into)?;

    Ok(api::ApiKeyServiceUpdateResponse {
        updated_at: Some(updated_at),
    })
}

pub async fn regenerate(
    req: api::ApiKeyServiceRegenerateRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ApiKeyServiceRegenerateResponse, Error> {
    let key_id = req.api_key_id.parse().map_err(Error::ParseKeyId)?;
    let existing = ApiKey::by_id(key_id, &mut write).await?;
    write
        .auth_for(&meta, ApiKeyPerm::Regenerate, &existing)
        .await?;

    let new_key = NewApiKey::regenerate(key_id, &mut write).await?;
    let updated_at = new_key.api_key.updated_at.ok_or(Error::MissingUpdatedAt)?;

    Ok(api::ApiKeyServiceRegenerateResponse {
        api_key: Some(new_key.secret.into()),
        updated_at: Some(NanosUtc::from(updated_at).into()),
    })
}

pub async fn delete(
    req: api::ApiKeyServiceDeleteRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ApiKeyServiceDeleteResponse, Error> {
    let key_id = req.api_key_id.parse().map_err(Error::ParseKeyId)?;
    let existing = ApiKey::by_id(key_id, &mut write).await?;
    write.auth_for(&meta, ApiKeyPerm::Delete, &existing).await?;

    ApiKey::delete(key_id, &mut write).await?;

    Ok(api::ApiKeyServiceDeleteResponse {})
}

impl From<ApiKey> for api::ListApiKey {
    fn from(api_key: ApiKey) -> Self {
        let resource = Resource::from(&api_key);
        api::ListApiKey {
            api_key_id: Some(api_key.id.to_string()),
            label: Some(api_key.label),
            resource: Some(common::Resource::from(resource)),
            created_at: Some(NanosUtc::from(api_key.created_at).into()),
            updated_at: api_key.updated_at.map(NanosUtc::from).map(Into::into),
        }
    }
}
