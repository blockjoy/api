use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::endpoint::Endpoint;
use crate::auth::resource::ResourceEntry;
use crate::config::Context;
use crate::database::{Conn, Transaction};
use crate::models::api_key::{ApiKey, ApiResource, NewApiKey, UpdateLabel, UpdateScope};
use crate::timestamp::NanosUtc;

use super::api::{self, api_key_service_server::ApiKeyService};
use super::Grpc;

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
        error!("{}: {err}", std::any::type_name::<Error>());

        use Error::*;
        match err {
            Auth(_) | Claims(_) | ClaimsNotUser => Status::permission_denied("Access denied."),
            Model(_) | Diesel(_) | MissingUpdatedAt => Status::internal("Internal error."),
            ParseKeyId(_) => Status::invalid_argument("id"),
            MissingCreateScope => Status::invalid_argument("scope"),
            ParseApiResource(_) => Status::invalid_argument("resource"),
            MissingScopeResourceId | ParseResourceId(_) => Status::invalid_argument("resource_id"),
            NothingToUpdate => Status::failed_precondition("Nothing to update."),
        }
    }
}

#[tonic::async_trait]
impl ApiKeyService for Grpc {
    async fn create(
        &self,
        req: Request<api::CreateApiKeyRequest>,
    ) -> super::Resp<api::CreateApiKeyResponse> {
        self.context
            .write(|conn, ctx| create(req, conn, ctx).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::ListApiKeyRequest>,
    ) -> super::Resp<api::ListApiKeyResponse> {
        self.context
            .read(|conn, ctx| list(req, conn, ctx).scope_boxed())
            .await
    }

    async fn update(
        &self,
        req: Request<api::UpdateApiKeyRequest>,
    ) -> super::Resp<api::UpdateApiKeyResponse> {
        self.context
            .write(|conn, ctx| update(req, conn, ctx).scope_boxed())
            .await
    }

    async fn regenerate(
        &self,
        req: Request<api::RegenerateApiKeyRequest>,
    ) -> super::Resp<api::RegenerateApiKeyResponse> {
        self.context
            .write(|conn, ctx| regenerate(req, conn, ctx).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::DeleteApiKeyRequest>,
    ) -> super::Resp<api::DeleteApiKeyResponse> {
        self.context
            .write(|conn, ctx| delete(req, conn, ctx).scope_boxed())
            .await
    }
}

async fn create(
    req: Request<api::CreateApiKeyRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Resp<api::CreateApiKeyResponse, Error> {
    let claims = ctx.claims(&req, Endpoint::ApiKeyCreate).await?;

    let req = req.into_inner();
    let scope = req.scope.ok_or(Error::MissingCreateScope)?;

    let entry = ResourceEntry::try_from(scope)?;
    let ensure = claims.ensure_admin(entry.into(), conn).await?;
    let user_id = ensure.user().ok_or(Error::ClaimsNotUser)?.user_id();

    let created = NewApiKey::create(user_id, req.label, entry, conn, ctx).await?;

    let resp = api::CreateApiKeyResponse {
        api_key: Some(created.secret.into()),
        created_at: Some(NanosUtc::from(created.api_key.created_at).into()),
    };
    Ok(Response::new(resp))
}

async fn list(
    req: Request<api::ListApiKeyRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Resp<api::ListApiKeyResponse, Error> {
    let claims = ctx.claims(&req, Endpoint::ApiKeyList).await?;
    let user_id = claims.resource().user().ok_or(Error::ClaimsNotUser)?;

    let keys = ApiKey::find_by_user(user_id, conn).await?;
    let api_keys = keys.into_iter().map(api::ListApiKey::from_model).collect();

    let resp = api::ListApiKeyResponse { api_keys };
    Ok(Response::new(resp))
}

async fn update(
    req: Request<api::UpdateApiKeyRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Resp<api::UpdateApiKeyResponse, Error> {
    let claims = ctx.claims(&req, Endpoint::ApiKeyUpdate).await?;

    let req = req.into_inner();
    let key_id = req.id.parse().map_err(Error::ParseKeyId)?;

    let existing = ApiKey::find_by_id(key_id, conn).await?;
    let entry = ResourceEntry::from(&existing);
    let _ = claims.ensure_admin(entry.into(), conn).await?;

    let mut updated_at = None;

    if let Some(label) = req.label {
        updated_at = UpdateLabel::new(key_id, label)
            .update(conn)
            .await
            .map(Some)?;
    }

    if let Some(scope) = req.scope {
        let entry = ResourceEntry::try_from(scope)?;
        updated_at = UpdateScope::new(key_id, entry)
            .update(conn)
            .await
            .map(Some)?;
    }

    let updated_at = updated_at
        .ok_or(Error::NothingToUpdate)
        .map(NanosUtc::from)
        .map(Into::into)?;

    let resp = api::UpdateApiKeyResponse {
        updated_at: Some(updated_at),
    };
    Ok(Response::new(resp))
}

async fn regenerate(
    req: Request<api::RegenerateApiKeyRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Resp<api::RegenerateApiKeyResponse, Error> {
    let claims = ctx.claims(&req, Endpoint::ApiKeyRegenerate).await?;

    let req = req.into_inner();
    let key_id = req.id.parse().map_err(Error::ParseKeyId)?;

    let existing = ApiKey::find_by_id(key_id, conn).await?;
    let entry = ResourceEntry::from(&existing);
    let _ = claims.ensure_admin(entry.into(), conn).await?;

    let new_key = NewApiKey::regenerate(key_id, conn, ctx).await?;
    let updated_at = new_key.api_key.updated_at.ok_or(Error::MissingUpdatedAt)?;

    let resp = api::RegenerateApiKeyResponse {
        api_key: Some(new_key.secret.into()),
        updated_at: Some(NanosUtc::from(updated_at).into()),
    };
    Ok(Response::new(resp))
}

async fn delete(
    req: Request<api::DeleteApiKeyRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Resp<api::DeleteApiKeyResponse, Error> {
    let claims = ctx.claims(&req, Endpoint::ApiKeyDelete).await?;

    let req = req.into_inner();
    let key_id = req.id.parse().map_err(Error::ParseKeyId)?;

    let existing = ApiKey::find_by_id(key_id, conn).await?;
    let entry = ResourceEntry::from(&existing);
    let _ = claims.ensure_admin(entry.into(), conn).await?;

    ApiKey::delete(key_id, conn).await?;

    let resp = api::DeleteApiKeyResponse {};
    Ok(Response::new(resp))
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
