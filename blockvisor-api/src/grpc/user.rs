use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::{
    UserAdminPerm, UserBillingPerm, UserPerm, UserSettingsAdminPerm, UserSettingsPerm,
};
use crate::auth::resource::UserId;
use crate::auth::{self, token, Authorize};
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::model::user::setting::{NewUserSetting, UserSetting};
use crate::model::user::{NewUser, UpdateUser, User, UserFilter, UserSearch, UserSort};
use crate::util::NanosUtc;

use super::api::user_service_server::UserService;
use super::{api, Grpc, Metadata, Status};

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
    Model(#[from] crate::model::user::Error),
    /// User search failed: {0}
    SearchOperator(crate::util::search::Error),
    /// Sort order: {0}
    SortOrder(crate::util::search::Error),
    /// The requested sort field is unknown.
    UnknownSortField,
    /// User settings error: {0}
    UserSettings(#[from] crate::model::user::setting::Error),
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
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Model(err) => err.into(),
            UserSettings(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl UserService for Grpc {
    async fn create(
        &self,
        req: Request<api::UserServiceCreateRequest>,
    ) -> Result<Response<api::UserServiceCreateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: Request<api::UserServiceGetRequest>,
    ) -> Result<Response<api::UserServiceGetResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::UserServiceListRequest>,
    ) -> Result<Response<api::UserServiceListResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn update(
        &self,
        req: Request<api::UserServiceUpdateRequest>,
    ) -> Result<Response<api::UserServiceUpdateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::UserServiceDeleteRequest>,
    ) -> Result<Response<api::UserServiceDeleteResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn get_billing(
        &self,
        req: Request<api::UserServiceGetBillingRequest>,
    ) -> Result<Response<api::UserServiceGetBillingResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_billing(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn update_billing(
        &self,
        req: Request<api::UserServiceUpdateBillingRequest>,
    ) -> Result<Response<api::UserServiceUpdateBillingResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_billing(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn delete_billing(
        &self,
        req: Request<api::UserServiceDeleteBillingRequest>,
    ) -> Result<Response<api::UserServiceDeleteBillingResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete_billing(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn get_settings(
        &self,
        req: Request<api::UserServiceGetSettingsRequest>,
    ) -> Result<Response<api::UserServiceGetSettingsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_settings(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn update_settings(
        &self,
        req: Request<api::UserServiceUpdateSettingsRequest>,
    ) -> Result<Response<api::UserServiceUpdateSettingsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_settings(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn delete_settings(
        &self,
        req: Request<api::UserServiceDeleteSettingsRequest>,
    ) -> Result<Response<api::UserServiceDeleteSettingsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete_settings(req, meta.into(), write).scope_boxed())
            .await
    }
}

pub async fn create(
    req: api::UserServiceCreateRequest,
    meta: Metadata,
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

pub async fn get(
    req: api::UserServiceGetRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::UserServiceGetResponse, Error> {
    let user_id: UserId = req.id.parse().map_err(Error::ParseId)?;
    read.auth_or_all(&meta, UserAdminPerm::Get, UserPerm::Get, user_id)
        .await?;

    let user = User::by_id(user_id, &mut read).await?;

    Ok(api::UserServiceGetResponse {
        user: Some(api::User::from_model(user)),
    })
}

pub async fn list(
    req: api::UserServiceListRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::UserServiceListResponse, Error> {
    let filter = req.into_filter()?;
    if let Some(org_id) = filter.org_id {
        read.auth_or_all(&meta, UserAdminPerm::Filter, UserPerm::Filter, org_id)
            .await?
    } else {
        read.auth_all(&meta, UserAdminPerm::Filter).await?
    };

    let (users, user_count) = filter.query(&mut read).await?;
    let users = api::User::from_models(users);

    Ok(api::UserServiceListResponse { users, user_count })
}

pub async fn update(
    req: api::UserServiceUpdateRequest,
    meta: Metadata,
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

pub async fn delete(
    req: api::UserServiceDeleteRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceDeleteResponse, Error> {
    let user_id: UserId = req.id.parse().map_err(Error::ParseId)?;
    write.auth(&meta, UserPerm::Delete, user_id).await?;

    User::delete(user_id, &mut write).await?;

    Ok(api::UserServiceDeleteResponse {})
}

pub async fn get_billing(
    req: api::UserServiceGetBillingRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::UserServiceGetBillingResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    read.auth(&meta, UserBillingPerm::Get, user_id).await?;

    let user = User::by_id(user_id, &mut read).await?;

    Ok(api::UserServiceGetBillingResponse {
        billing_id: user.chargebee_billing_id,
    })
}

pub async fn update_billing(
    req: api::UserServiceUpdateBillingRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceUpdateBillingResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    write.auth(&meta, UserBillingPerm::Update, user_id).await?;

    let mut user = User::by_id(user_id, &mut write).await?;
    user.chargebee_billing_id = req.billing_id;
    user.update(&mut write).await?;

    Ok(api::UserServiceUpdateBillingResponse {
        billing_id: user.chargebee_billing_id,
    })
}

pub async fn delete_billing(
    req: api::UserServiceDeleteBillingRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceDeleteBillingResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    write.auth(&meta, UserBillingPerm::Delete, user_id).await?;

    let user = User::by_id(user_id, &mut write).await?;
    user.delete_billing(&mut write).await?;

    Ok(api::UserServiceDeleteBillingResponse {})
}

pub async fn get_settings(
    req: api::UserServiceGetSettingsRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::UserServiceGetSettingsResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    read.auth_or_all(
        &meta,
        UserSettingsAdminPerm::Get,
        UserSettingsPerm::Get,
        user_id,
    )
    .await?;

    let user = User::by_id(user_id, &mut read).await?;
    let settings = UserSetting::by_user(user.id, &mut read)
        .await?
        .into_iter()
        .map(|s| (s.name, s.value))
        .collect();

    Ok(api::UserServiceGetSettingsResponse { settings })
}

pub async fn update_settings(
    req: api::UserServiceUpdateSettingsRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceUpdateSettingsResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    write
        .auth_or_all(
            &meta,
            UserSettingsAdminPerm::Update,
            UserSettingsPerm::Update,
            user_id,
        )
        .await?;

    let user = User::by_id(user_id, &mut write).await?;
    let setting = NewUserSetting::new(user.id, &req.name, &req.value)
        .create_or_update(&mut write)
        .await?;

    Ok(api::UserServiceUpdateSettingsResponse {
        name: setting.name,
        value: setting.value,
    })
}

pub async fn delete_settings(
    req: api::UserServiceDeleteSettingsRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceDeleteSettingsResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    write
        .auth_or_all(
            &meta,
            UserSettingsAdminPerm::Delete,
            UserSettingsPerm::Delete,
            user_id,
        )
        .await?;

    let user = User::by_id(user_id, &mut write).await?;
    UserSetting::delete(user.id, &req.name, &mut write).await?;

    Ok(api::UserServiceDeleteSettingsResponse {})
}

impl api::User {
    pub fn from_model(model: User) -> Self {
        Self {
            id: model.id.to_string(),
            email: model.email,
            first_name: model.first_name,
            last_name: model.last_name,
            created_at: Some(NanosUtc::from(model.created_at).into()),
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
    fn into_filter(self) -> Result<UserFilter, Error> {
        let org_id = self
            .org_id
            .map(|id| id.parse().map_err(Error::ParseOrgId))
            .transpose()?;
        let search = self
            .search
            .map(|search| {
                Ok::<_, Error>(UserSearch {
                    operator: search
                        .operator()
                        .try_into()
                        .map_err(Error::SearchOperator)?,
                    id: search.id.map(|id| id.trim().to_lowercase()),
                    name: search.name.map(|name| name.trim().to_lowercase()),
                    email: search.email.map(|email| email.trim().to_lowercase()),
                })
            })
            .transpose()?;
        let sort = self
            .sort
            .into_iter()
            .map(|sort| {
                let order = sort.order().try_into().map_err(Error::SortOrder)?;
                match sort.field() {
                    api::UserSortField::Unspecified => Err(Error::UnknownSortField),
                    api::UserSortField::Email => Ok(UserSort::Email(order)),
                    api::UserSortField::FirstName => Ok(UserSort::FirstName(order)),
                    api::UserSortField::LastName => Ok(UserSort::LastName(order)),
                    api::UserSortField::CreatedAt => Ok(UserSort::CreatedAt(order)),
                }
            })
            .collect::<Result<_, _>>()?;

        Ok(UserFilter {
            org_id,
            offset: self.offset,
            limit: self.limit,
            search,
            sort,
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
