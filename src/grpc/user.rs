use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::{UserAdminPerm, UserBillingPerm, UserPerm};
use crate::auth::resource::UserId;
use crate::auth::{self, token, Authorize};
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::models::user::{NewUser, UpdateUser, User, UserFilter};
use crate::timestamp::NanosUtc;

use super::api::user_service_server::UserService;
use super::{api, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// User email error: {0}
    Email(#[from] crate::email::Error),
    /// Failed to parse UserId: {0}
    ParseId(uuid::Error),
    /// Failed to parse invitation id: {0}
    ParseInvitationId(uuid::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse UserId: {0}
    ParseUserId(uuid::Error),
    /// User model error: {0}
    Model(#[from] crate::models::user::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | Email(_) | ParseInvitationId(_) => Status::internal("Internal error."),
            ParseId(_) => Status::invalid_argument("id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseUserId(_) => Status::invalid_argument("user_id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Model(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl UserService for Grpc {
    async fn create(
        &self,
        req: Request<api::UserServiceCreateRequest>,
    ) -> Result<Response<api::UserServiceCreateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta, write).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: Request<api::UserServiceGetRequest>,
    ) -> Result<Response<api::UserServiceGetResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta, read).scope_boxed()).await
    }

    async fn list(
        &self,
        req: Request<api::UserServiceListRequest>,
    ) -> Result<Response<api::UserServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }

    async fn update(
        &self,
        req: Request<api::UserServiceUpdateRequest>,
    ) -> Result<Response<api::UserServiceUpdateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta, write).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::UserServiceDeleteRequest>,
    ) -> Result<Response<api::UserServiceDeleteResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta, write).scope_boxed())
            .await
    }

    async fn get_billing(
        &self,
        req: Request<api::UserServiceGetBillingRequest>,
    ) -> Result<Response<api::UserServiceGetBillingResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_billing(req, meta, read).scope_boxed())
            .await
    }

    async fn update_billing(
        &self,
        req: Request<api::UserServiceUpdateBillingRequest>,
    ) -> Result<Response<api::UserServiceUpdateBillingResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_billing(req, meta, write).scope_boxed())
            .await
    }

    async fn delete_billing(
        &self,
        req: Request<api::UserServiceDeleteBillingRequest>,
    ) -> Result<Response<api::UserServiceDeleteBillingResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete_billing(req, meta, write).scope_boxed())
            .await
    }
}

async fn create(
    req: api::UserServiceCreateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceCreateResponse, Error> {
    // This endpoint does not necessarily require authentication, since this is where users first
    // register.
    let invitation_id = match write.auth_all(&meta, UserPerm::Create).await {
        // If there is a successful authorization, then we get the invitation id from it if there is
        // one.
        Ok(authz) => authz
            .get_data("invitation_id")
            .map(|tkn| tkn.parse().map_err(Error::ParseInvitationId))
            .transpose()?,
        // If we cannot construct authorization because no token is present, then we cannot produce
        // an invitation id, but we still allow the user create process to continue.
        Err(auth::Error::ParseRequestToken(token::Error::MissingAuthHeader)) => None,
        // If the constructing the authorization failed for another reason, we report this error
        // back.
        Err(e) => return Err(e.into()),
    };

    let new_user = req.as_new()?.create(&mut write).await?;
    write
        .ctx
        .email
        .registration_confirmation(&new_user, invitation_id)
        .await?;

    Ok(api::UserServiceCreateResponse {
        user: Some(api::User::from_model(new_user)),
    })
}

async fn get(
    req: api::UserServiceGetRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::UserServiceGetResponse, Error> {
    let user_id: UserId = req.id.parse().map_err(Error::ParseId)?;
    read.auth_or_all(&meta, UserAdminPerm::Get, UserPerm::Get, user_id)
        .await?;

    let user = User::find_by_id(user_id, &mut read).await?;

    Ok(api::UserServiceGetResponse {
        user: Some(api::User::from_model(user)),
    })
}

async fn list(
    req: api::UserServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::UserServiceListResponse, Error> {
    let filter = req.as_filter()?;
    if let Some(org_id) = filter.org_id {
        read.auth_or_all(&meta, UserAdminPerm::Filter, UserPerm::Filter, org_id)
            .await?
    } else {
        read.auth_all(&meta, UserAdminPerm::Filter).await?
    };

    let (user_count, users) = User::filter(filter, &mut read).await?;
    let users = api::User::from_models(users);
    Ok(api::UserServiceListResponse { users, user_count })
}

async fn update(
    req: api::UserServiceUpdateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceUpdateResponse, Error> {
    let user_id: UserId = req.id.parse().map_err(Error::ParseId)?;
    write
        .auth_or_all(&meta, UserAdminPerm::Update, UserPerm::Update, user_id)
        .await?;

    let user = req.as_update(user_id).update(&mut write).await?;

    Ok(api::UserServiceUpdateResponse {
        user: Some(api::User::from_model(user)),
    })
}

async fn delete(
    req: api::UserServiceDeleteRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceDeleteResponse, Error> {
    let user_id: UserId = req.id.parse().map_err(Error::ParseId)?;
    write.auth(&meta, UserPerm::Delete, user_id).await?;

    User::delete(user_id, &mut write).await?;

    Ok(api::UserServiceDeleteResponse {})
}

async fn get_billing(
    req: api::UserServiceGetBillingRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::UserServiceGetBillingResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    read.auth(&meta, UserBillingPerm::Get, user_id).await?;

    let user = User::find_by_id(user_id, &mut read).await?;

    Ok(api::UserServiceGetBillingResponse {
        billing_id: user.billing_id,
    })
}

async fn update_billing(
    req: api::UserServiceUpdateBillingRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceUpdateBillingResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    write.auth(&meta, UserBillingPerm::Update, user_id).await?;

    let mut user = User::find_by_id(user_id, &mut write).await?;
    user.billing_id = req.billing_id;
    user.update(&mut write).await?;

    Ok(api::UserServiceUpdateBillingResponse {
        billing_id: user.billing_id,
    })
}

async fn delete_billing(
    req: api::UserServiceDeleteBillingRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceDeleteBillingResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    write.auth(&meta, UserBillingPerm::Delete, user_id).await?;

    let user = User::find_by_id(user_id, &mut write).await?;
    user.delete_billing(&mut write).await?;

    Ok(api::UserServiceDeleteBillingResponse {})
}

impl api::User {
    pub fn from_model(model: User) -> Self {
        Self {
            id: model.id.to_string(),
            email: model.email,
            first_name: model.first_name,
            last_name: model.last_name,
            created_at: Some(NanosUtc::from(model.created_at).into()),
            updated_at: None,
        }
    }

    pub fn from_models(models: Vec<User>) -> Vec<Self> {
        models.into_iter().map(Self::from_model).collect()
    }
}

impl api::UserServiceCreateRequest {
    fn as_new(&self) -> Result<NewUser<'_>, Error> {
        NewUser::new(
            &self.email,
            &self.first_name,
            &self.last_name,
            &self.password,
        )
        .map_err(Into::into)
    }
}

impl api::UserServiceListRequest {
    fn as_filter(&self) -> Result<UserFilter<'_>, Error> {
        Ok(UserFilter {
            org_id: self
                .org_id
                .as_ref()
                .map(|id| id.parse().map_err(Error::ParseOrgId))
                .transpose()?,
            email_like: self.email_like.as_deref(),
            offset: self.offset,
            limit: self.limit,
        })
    }
}

impl api::UserServiceUpdateRequest {
    pub fn as_update(&self, id: UserId) -> UpdateUser<'_> {
        UpdateUser {
            id,
            first_name: self.first_name.as_deref(),
            last_name: self.last_name.as_deref(),
        }
    }
}
