use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::SubscriptionPerm;
use crate::auth::resource::OrgId;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::model::org::Org;
use crate::model::subscription::{NewSubscription, Subscription};

use super::api::subscription_service_server::SubscriptionService;
use super::{api, Grpc, Status};

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
    /// Missing `org_id`.
    MissingOrgId,
    /// Missing `user_id`.
    MissingUserId,
    /// Subscription model error: {0}
    Model(#[from] crate::model::subscription::Error),
    /// Subscription org error: {0}
    Org(#[from] crate::model::org::Error),
    /// Failed to parse SubscriptionId: {0}
    ParseId(uuid::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse UserId: {0}
    ParseUserId(uuid::Error),
    /// Requested user does not match token user.
    UserMismatch,
    /// User is not in the requested org.
    UserNotInOrg,
}

impl super::ResponseError for Error {
    fn report(&self) -> Status {
        use Error::*;
        error!("{self}");
        match self {
            ClaimsNotUser | UserMismatch | UserNotInOrg => Status::forbidden("Access denied."),
            Diesel(_) => Status::internal("Internal error."),
            MissingUserId | ParseUserId(_) => Status::invalid_argument("user_id"),
            MissingOrgId | ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseId(_) => Status::invalid_argument("id"),
            Auth(err) => err.report(),
            Claims(err) => err.report(),
            Model(err) => err.report(),
            Org(err) => err.report(),
        }
    }
}

#[tonic::async_trait]
impl SubscriptionService for Grpc {
    async fn create(
        &self,
        req: Request<api::SubscriptionServiceCreateRequest>,
    ) -> Result<Response<api::SubscriptionServiceCreateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: Request<api::SubscriptionServiceGetRequest>,
    ) -> Result<Response<api::SubscriptionServiceGetResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::SubscriptionServiceListRequest>,
    ) -> Result<Response<api::SubscriptionServiceListResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn update(
        &self,
        req: Request<api::SubscriptionServiceUpdateRequest>,
    ) -> Result<Response<api::SubscriptionServiceUpdateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|read| update(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::SubscriptionServiceDeleteRequest>,
    ) -> Result<Response<api::SubscriptionServiceDeleteResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta.into(), write).scope_boxed())
            .await
    }
}

async fn create(
    req: api::SubscriptionServiceCreateRequest,
    meta: super::NaiveMeta,
    mut write: WriteConn<'_, '_>,
) -> Result<api::SubscriptionServiceCreateResponse, Error> {
    let org_id = req.org_id.parse().map_err(Error::ParseOrgId)?;
    let authz = write.auth(&meta, SubscriptionPerm::Create, org_id).await?;

    let user_id = req.user_id.parse().map_err(Error::ParseUserId)?;
    let auth_user_id = authz.resource().user().ok_or(Error::ClaimsNotUser)?;
    if user_id != auth_user_id {
        return Err(Error::UserMismatch);
    } else if !Org::has_user(org_id, user_id, &mut write).await? {
        return Err(Error::UserNotInOrg);
    }

    let sub = NewSubscription::new(org_id, user_id, req.external_id);
    let created = sub.create(&mut write).await?;

    Ok(api::SubscriptionServiceCreateResponse {
        subscription: Some(api::Subscription::from_model(created)),
    })
}

async fn get(
    req: api::SubscriptionServiceGetRequest,
    meta: super::NaiveMeta,
    mut read: ReadConn<'_, '_>,
) -> Result<api::SubscriptionServiceGetResponse, Error> {
    let org_id = req.org_id.parse().map_err(Error::ParseOrgId)?;
    read.auth(&meta, SubscriptionPerm::Get, org_id).await?;

    let sub = Subscription::by_org_id(org_id, &mut read).await?;

    Ok(api::SubscriptionServiceGetResponse {
        subscription: sub.map(api::Subscription::from_model),
    })
}

async fn list(
    req: api::SubscriptionServiceListRequest,
    meta: super::NaiveMeta,
    mut read: ReadConn<'_, '_>,
) -> Result<api::SubscriptionServiceListResponse, Error> {
    let user_id = req.user_id.ok_or(Error::MissingUserId)?;
    let user_id = user_id.parse().map_err(Error::ParseUserId)?;
    read.auth(&meta, SubscriptionPerm::List, user_id).await?;

    let subscriptions = Subscription::by_user_id(user_id, &mut read)
        .await?
        .into_iter()
        .map(api::Subscription::from_model)
        .collect();

    Ok(api::SubscriptionServiceListResponse { subscriptions })
}

// Note that for now this just checks if a permission is available.
async fn update(
    req: api::SubscriptionServiceUpdateRequest,
    meta: super::NaiveMeta,
    mut write: WriteConn<'_, '_>,
) -> Result<api::SubscriptionServiceUpdateResponse, Error> {
    let org_id = req.org_id.ok_or(Error::MissingOrgId)?;
    let org_id: OrgId = org_id.parse().map_err(Error::ParseOrgId)?;
    write.auth(&meta, SubscriptionPerm::Update, org_id).await?;

    Ok(api::SubscriptionServiceUpdateResponse {})
}

async fn delete(
    req: api::SubscriptionServiceDeleteRequest,
    meta: super::NaiveMeta,
    mut write: WriteConn<'_, '_>,
) -> Result<api::SubscriptionServiceDeleteResponse, Error> {
    let sub_id = req.id.parse().map_err(Error::ParseId)?;
    let sub = Subscription::by_id(sub_id, &mut write).await?;

    write
        .auth(&meta, SubscriptionPerm::Delete, sub.org_id)
        .await?;

    Subscription::delete(sub_id, &mut write).await?;

    Ok(api::SubscriptionServiceDeleteResponse {})
}

impl api::Subscription {
    pub fn from_model(model: Subscription) -> Self {
        api::Subscription {
            id: model.id.to_string(),
            org_id: model.org_id.to_string(),
            user_id: model.user_id.to_string(),
            external_id: model.external_id,
        }
    }
}
