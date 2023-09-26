use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::SubscriptionPerm;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::models::org::Org;
use crate::models::subscription::{NewSubscription, Subscription};

use super::api::subscription_service_server::SubscriptionService;
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
    /// Missing `user_id`.
    MissingUserId,
    /// Subscription model error: {0}
    Model(#[from] crate::models::subscription::Error),
    /// Subscription org error: {0}
    Org(#[from] crate::models::org::Error),
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

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!("{err}");
        use Error::*;
        match err {
            ClaimsNotUser | UserMismatch | UserNotInOrg => {
                Status::permission_denied("Access denied.")
            }
            Diesel(_) => Status::internal("Internal error."),
            MissingUserId | ParseUserId(_) => Status::invalid_argument("user_id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseId(_) => Status::invalid_argument("id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Model(err) => err.into(),
            Org(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl SubscriptionService for Grpc {
    async fn create(
        &self,
        req: Request<api::SubscriptionServiceCreateRequest>,
    ) -> Result<Response<api::SubscriptionServiceCreateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta, write).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: Request<api::SubscriptionServiceGetRequest>,
    ) -> Result<Response<api::SubscriptionServiceGetResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta, read).scope_boxed()).await
    }

    async fn list(
        &self,
        req: Request<api::SubscriptionServiceListRequest>,
    ) -> Result<Response<api::SubscriptionServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }

    async fn delete(
        &self,
        req: Request<api::SubscriptionServiceDeleteRequest>,
    ) -> Result<Response<api::SubscriptionServiceDeleteResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta, write).scope_boxed())
            .await
    }
}

async fn create(
    req: api::SubscriptionServiceCreateRequest,
    meta: MetadataMap,
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
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::SubscriptionServiceGetResponse, Error> {
    let org_id = req.org_id.parse().map_err(Error::ParseOrgId)?;
    let _ = read.auth(&meta, SubscriptionPerm::Get, org_id).await?;

    let sub = Subscription::find_by_org(org_id, &mut read).await?;

    Ok(api::SubscriptionServiceGetResponse {
        subscription: sub.map(api::Subscription::from_model),
    })
}

async fn list(
    req: api::SubscriptionServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::SubscriptionServiceListResponse, Error> {
    let user_id = req
        .user_id
        .ok_or(Error::MissingUserId)?
        .parse()
        .map_err(Error::ParseUserId)?;
    let _ = read.auth(&meta, SubscriptionPerm::List, user_id).await?;

    let subscriptions = Subscription::find_by_user(user_id, &mut read)
        .await?
        .into_iter()
        .map(api::Subscription::from_model)
        .collect();

    Ok(api::SubscriptionServiceListResponse { subscriptions })
}

async fn delete(
    req: api::SubscriptionServiceDeleteRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::SubscriptionServiceDeleteResponse, Error> {
    let sub_id = req.id.parse().map_err(Error::ParseId)?;
    let sub = Subscription::find_by_id(sub_id, &mut write).await?;

    let _ = write
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
