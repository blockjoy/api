use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::endpoint::Endpoint;
use crate::auth::resource::Resource;
use crate::models::subscription::{NewSubscription, Subscription};
use crate::models::{Conn, Org};

use super::api::{self, subscription_service_server};
use super::Grpc;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Failed to create new subscription: {0}
    CreateSub(crate::models::subscription::Error),
    /// Failed to delete subscription: {0}
    DeleteSub(crate::models::subscription::Error),
    /// Access denied.
    Denied,
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Failed to find subscription: {0}
    FindById(crate::models::subscription::Error),
    /// Failed to find org subscription: {0}
    FindByOrg(crate::models::subscription::Error),
    /// Missing `user_id`.
    MissingUserId,
    /// Failed to check if user is a member of an Org: {0}
    OrgMember(crate::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse SubscriptionId: {0}
    ParseSubId(uuid::Error),
    /// Failed to parse UserId: {0}
    ParseUserId(uuid::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!("{}: {err}", std::any::type_name::<Error>());

        use Error::*;
        match err {
            Auth(_) | Claims(_) | Denied => Status::permission_denied("Access denied."),
            CreateSub(_) | DeleteSub(_) | Diesel(_) | FindById(_) | FindByOrg(_) | OrgMember(_) => {
                Status::internal("Internal error.")
            }
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            MissingUserId | ParseUserId(_) => Status::invalid_argument("user_id"),
            ParseSubId(_) => Status::invalid_argument("id"),
        }
    }
}

#[tonic::async_trait]
impl subscription_service_server::SubscriptionService for Grpc {
    async fn create(
        &self,
        req: Request<api::SubscriptionServiceCreateRequest>,
    ) -> super::Resp<api::SubscriptionServiceCreateResponse> {
        self.trx(|c| create(req, c).scope_boxed()).await
    }

    async fn get(
        &self,
        req: Request<api::SubscriptionServiceGetRequest>,
    ) -> super::Resp<api::SubscriptionServiceGetResponse> {
        self.run(|c| get(req, c).scope_boxed()).await
    }

    async fn list(
        &self,
        req: Request<api::SubscriptionServiceListRequest>,
    ) -> super::Resp<api::SubscriptionServiceListResponse> {
        self.run(|c| list(req, c).scope_boxed()).await
    }

    async fn delete(
        &self,
        req: Request<api::SubscriptionServiceDeleteRequest>,
    ) -> super::Resp<api::SubscriptionServiceDeleteResponse> {
        self.trx(|tx| delete(req, tx).scope_boxed()).await
    }
}

async fn create(
    req: Request<api::SubscriptionServiceCreateRequest>,
    conn: &mut Conn,
) -> super::Resp<api::SubscriptionServiceCreateResponse, Error> {
    let claims = conn.claims(&req, Endpoint::SubscriptionCreate).await?;

    let req = req.into_inner();
    let org_id = req.org_id.parse().map_err(Error::ParseOrgId)?;
    let user_id = req.user_id.parse().map_err(Error::ParseUserId)?;

    match claims.resource() {
        Resource::User(id) if id == user_id => Org::is_member(id, org_id, conn)
            .await
            .map_err(Error::OrgMember)?
            .then_some(())
            .ok_or(Error::Denied),
        _ => Err(Error::Denied),
    }?;

    let sub = NewSubscription::new(org_id, user_id, req.external_id);
    let created = sub.create(conn).await.map_err(Error::CreateSub)?;

    let resp = api::SubscriptionServiceCreateResponse {
        subscription: Some(api::Subscription::from_model(created)),
    };
    Ok(Response::new(resp))
}

async fn get(
    req: Request<api::SubscriptionServiceGetRequest>,
    conn: &mut Conn,
) -> super::Resp<api::SubscriptionServiceGetResponse, Error> {
    let claims = conn.claims(&req, Endpoint::SubscriptionGet).await?;

    let req = req.into_inner();
    let org_id = req.org_id.parse().map_err(Error::ParseOrgId)?;
    let _ = claims.ensure_org(org_id, false, conn).await?;

    let sub = Subscription::find_by_org(org_id, conn)
        .await
        .map_err(Error::FindByOrg)?;

    let resp = api::SubscriptionServiceGetResponse {
        subscription: sub.map(api::Subscription::from_model),
    };
    Ok(Response::new(resp))
}

async fn list(
    req: Request<api::SubscriptionServiceListRequest>,
    conn: &mut Conn,
) -> super::Resp<api::SubscriptionServiceListResponse, Error> {
    let claims = conn.claims(&req, Endpoint::SubscriptionList).await?;

    let req = req.into_inner();
    let user_id = req.user_id.ok_or(Error::MissingUserId)?;
    let user_id = user_id.parse().map_err(Error::ParseUserId)?;
    let _ = claims.ensure_user(user_id)?;

    let subscriptions = Subscription::find_by_user(user_id, conn)
        .await
        .map_err(Error::FindByOrg)?
        .into_iter()
        .map(api::Subscription::from_model)
        .collect();

    let resp = api::SubscriptionServiceListResponse { subscriptions };
    Ok(Response::new(resp))
}

async fn delete(
    req: Request<api::SubscriptionServiceDeleteRequest>,
    conn: &mut Conn,
) -> super::Resp<api::SubscriptionServiceDeleteResponse, Error> {
    let claims = conn.claims(&req, Endpoint::SubscriptionDelete).await?;

    let req = req.into_inner();
    let sub_id = req.id.parse().map_err(Error::ParseSubId)?;

    let existing = Subscription::find_by_id(sub_id, conn)
        .await
        .map_err(Error::FindById)?;

    match claims.resource() {
        Resource::User(id) if id == existing.user_id => Ok(()),
        Resource::Org(id) if id == existing.org_id => Ok(()),
        _ => Err(Error::Denied),
    }?;

    Subscription::delete(sub_id, conn)
        .await
        .map_err(Error::DeleteSub)?;

    let resp = api::SubscriptionServiceDeleteResponse {};
    Ok(Response::new(resp))
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
