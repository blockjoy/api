use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::ApiKeyPerm;
use crate::auth::resource::ResourceEntry;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::models::api_key::{ApiKey, ApiResource, NewApiKey, UpdateLabel, UpdateScope};
use crate::timestamp::NanosUtc;

use super::api::api_key_service_server::ApiKeyService;
use super::{api, Grpc};

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
    /// Create API key request missing scope.
    MissingCreateScope,
    /// ApiKeyScope missing `resource_id`.
    MissingScopeResourceId,
    /// Missing API key `updated_at`.
    MissingUpdatedAt,
    /// Database model error: {0}
    Model(#[from] crate::models::api_key::Error),
    /// Nothing is set to be updated in the request.
    NothingToUpdate,
    /// Parse ApiResource: {0}
    ParseApiResource(crate::models::api_key::Error),
    /// Failed to parse KeyId: {0}
    ParseKeyId(crate::auth::token::api_key::Error),
    /// Failed to parse ResourceId: {0}
    ParseResourceId(uuid::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            ClaimsNotUser => Status::permission_denied("Access denied."),
            Diesel(_) | MissingUpdatedAt => Status::internal("Internal error."),
            ParseKeyId(_) => Status::invalid_argument("id"),
            MissingCreateScope => Status::invalid_argument("scope"),
            ParseApiResource(_) => Status::invalid_argument("resource"),
            MissingScopeResourceId | ParseResourceId(_) => Status::invalid_argument("resource_id"),
            NothingToUpdate => Status::failed_precondition("Nothing to update."),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Model(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl ApiKeyService for Grpc {
    async fn create(
        &self,
        req: Request<api::ApiKeyServiceCreateRequest>,
    ) -> Result<Response<api::ApiKeyServiceCreateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta, write).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::ApiKeyServiceListRequest>,
    ) -> Result<Response<api::ApiKeyServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }

    async fn update(
        &self,
        req: Request<api::ApiKeyServiceUpdateRequest>,
    ) -> Result<Response<api::ApiKeyServiceUpdateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta, write).scope_boxed())
            .await
    }

    async fn regenerate(
        &self,
        req: Request<api::ApiKeyServiceRegenerateRequest>,
    ) -> Result<Response<api::ApiKeyServiceRegenerateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| regenerate(req, meta, write).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::ApiKeyServiceDeleteRequest>,
    ) -> Result<Response<api::ApiKeyServiceDeleteResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta, write).scope_boxed())
            .await
    }
}

async fn create(
    req: api::ApiKeyServiceCreateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ApiKeyServiceCreateResponse, Error> {
    let scope = req.scope.ok_or(Error::MissingCreateScope)?;
    let entry = ResourceEntry::try_from(scope)?;

    let authz = write.auth(&meta, ApiKeyPerm::Create, entry).await?;
    let user_id = authz.resource().user().ok_or(Error::ClaimsNotUser)?;

    let created = NewApiKey::create(user_id, req.label, entry, &mut write).await?;

    Ok(api::ApiKeyServiceCreateResponse {
        api_key: Some(created.secret.into()),
        created_at: Some(NanosUtc::from(created.api_key.created_at).into()),
    })
}

async fn list(
    _req: api::ApiKeyServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ApiKeyServiceListResponse, Error> {
    let authz = read.auth_all(&meta, ApiKeyPerm::List).await?;
    let user_id = authz.resource().user().ok_or(Error::ClaimsNotUser)?;

    let keys = ApiKey::find_by_user(user_id, &mut read).await?;
    let api_keys = keys.into_iter().map(api::ListApiKey::from_model).collect();

    Ok(api::ApiKeyServiceListResponse { api_keys })
}

async fn update(
    req: api::ApiKeyServiceUpdateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ApiKeyServiceUpdateResponse, Error> {
    let key_id = req.id.parse().map_err(Error::ParseKeyId)?;
    let existing = ApiKey::find_by_id(key_id, &mut write).await?;

    let entry = ResourceEntry::from(&existing);
    write.auth(&meta, ApiKeyPerm::Update, entry).await?;

    let mut updated_at = None;

    if let Some(label) = req.label {
        updated_at = UpdateLabel::new(key_id, label)
            .update(&mut write)
            .await
            .map(Some)?;
    }

    if let Some(scope) = req.scope {
        updated_at = UpdateScope::new(key_id, scope.try_into()?)
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

async fn regenerate(
    req: api::ApiKeyServiceRegenerateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ApiKeyServiceRegenerateResponse, Error> {
    let key_id = req.id.parse().map_err(Error::ParseKeyId)?;
    let existing = ApiKey::find_by_id(key_id, &mut write).await?;
    let entry = ResourceEntry::from(&existing);

    write.auth(&meta, ApiKeyPerm::Regenerate, entry).await?;

    let new_key = NewApiKey::regenerate(key_id, &mut write).await?;
    let updated_at = new_key.api_key.updated_at.ok_or(Error::MissingUpdatedAt)?;

    Ok(api::ApiKeyServiceRegenerateResponse {
        api_key: Some(new_key.secret.into()),
        updated_at: Some(NanosUtc::from(updated_at).into()),
    })
}

async fn delete(
    req: api::ApiKeyServiceDeleteRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ApiKeyServiceDeleteResponse, Error> {
    let key_id = req.id.parse().map_err(Error::ParseKeyId)?;
    let existing = ApiKey::find_by_id(key_id, &mut write).await?;
    let entry = ResourceEntry::from(&existing);

    write.auth(&meta, ApiKeyPerm::Delete, entry).await?;

    ApiKey::delete(key_id, &mut write).await?;

    Ok(api::ApiKeyServiceDeleteResponse {})
}

impl api::ListApiKey {
    fn from_model(api_key: ApiKey) -> Self {
        let scope = api::ApiKeyScope::from_model(&api_key);

        api::ListApiKey {
            id: Some(format!("{}", *api_key.id)),
            label: Some(api_key.label),
            scope: Some(scope),
            created_at: Some(NanosUtc::from(api_key.created_at).into()),
            updated_at: api_key.updated_at.map(NanosUtc::from).map(Into::into),
        }
    }
}

impl api::ApiKeyScope {
    fn from_model(api_key: &ApiKey) -> Self {
        api::ApiKeyScope {
            resource: api_key.resource as i32,
            resource_id: Some(format!("{}", *api_key.resource_id)),
        }
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn from_entry(entry: ResourceEntry) -> Self {
        api::ApiKeyScope {
            resource: ApiResource::from(entry.resource_type) as i32,
            resource_id: Some(format!("{}", *entry.resource_id)),
        }
    }
}

impl TryFrom<api::ApiKeyScope> for ResourceEntry {
    type Error = Error;

    fn try_from(scope: api::ApiKeyScope) -> Result<Self, Self::Error> {
        let api_resource =
            ApiResource::try_from(scope.resource).map_err(Error::ParseApiResource)?;
        let resource_type = api_resource.into();

        let resource_id = scope
            .resource_id
            .ok_or(Error::MissingScopeResourceId)?
            .parse()
            .map_err(Error::ParseResourceId)?;

        Ok(ResourceEntry {
            resource_type,
            resource_id,
        })
    }
}
