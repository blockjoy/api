use std::collections::HashSet;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::claims::Granted;
use crate::auth::rbac::{ApiKeyPerm, Perm};
use crate::auth::resource::Resource;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::model::api_key::{ApiKey, NewApiKey};
use crate::model::sql::Permissions;
use crate::util::NanosUtc;

use super::api::api_key_service_server::ApiKeyService;
use super::{api, Grpc, Metadata, Status};

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
    /// Database model error: {0}
    Model(#[from] crate::model::api_key::Error),
    /// Failed to parse KeyId: {0}
    ParseId(crate::auth::token::api_key::Error),
    /// Failed to parse Perm: {0}
    ParsePerm(String),
    /// API key resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// API key sql type error: {0}
    Sql(#[from] crate::model::sql::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) => Status::internal("Internal error."),
            ClaimsNotUser => Status::forbidden("Access denied."),
            MissingResource => Status::invalid_argument("resource"),
            ParseId(_) => Status::invalid_argument("api_key_id"),
            ParsePerm(_) => Status::invalid_argument("permission"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Model(err) => err.into(),
            Resource(err) => err.into(),
            Sql(err) => err.into(),
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
    let org_id = resource.org_id(&mut write).await?;
    let perms = req
        .permissions
        .iter()
        .map(|perm| perm.parse().map_err(Error::ParsePerm))
        .collect::<Result<HashSet<Perm>, _>>()?;

    // first get the user permissions for the org
    let granted = Granted::for_org(user_id, org_id, true, &mut write).await?;
    // then append additional permissions from the token
    let granted = Granted::from_access(&authz.claims.access, Some(granted), &mut write).await?;
    // then filter by the requested api key permissions
    let granted = granted.ensure_all_perms(perms, resource)?;

    let permissions = Permissions::from(granted);
    let created = NewApiKey::create(user_id, req.label, resource, permissions, &mut write).await?;

    Ok(api::ApiKeyServiceCreateResponse {
        api_key: created.secret.into(),
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
    let api_keys = keys.into_iter().map(Into::into).collect();

    Ok(api::ApiKeyServiceListResponse { api_keys })
}

pub async fn delete(
    req: api::ApiKeyServiceDeleteRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ApiKeyServiceDeleteResponse, Error> {
    let key_id = req.api_key_id.parse().map_err(Error::ParseId)?;
    let existing = ApiKey::by_id(key_id, &mut write).await?;
    write
        .auth_for(&meta, ApiKeyPerm::Delete, existing.user_id)
        .await?;

    ApiKey::delete(key_id, &mut write).await?;

    Ok(api::ApiKeyServiceDeleteResponse {})
}
