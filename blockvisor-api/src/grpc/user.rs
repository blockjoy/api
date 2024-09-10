use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::{UserAdminPerm, UserPerm, UserSettingsAdminPerm, UserSettingsPerm};
use crate::auth::resource::UserId;
use crate::auth::{self, token, Authorize};
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::model::user::setting::{NewUserSetting, UserSetting};
use crate::model::user::{NewUser, UpdateUser, User, UserFilter, UserSearch, UserSort};

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
    /// User search failed: {0}
    SearchOperator(crate::util::search::Error),
    /// Sort order: {0}
    SortOrder(crate::util::search::Error),
    /// The requested sort field is unknown.
    UnknownSortField,
    /// User model error: {0}
    User(#[from] crate::model::user::Error),
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
            User(err) => err.into(),
            UserSettings(_) => err.into(),
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
    // A successful authz is not needed when users first register
    let invitation_id = match write.auth(&meta, UserPerm::Create).await {
        // If there is a successful authorization, then get the invitation id
        Ok(authz) => authz
            .get_data("invitation_id")
            .map(|id| id.parse().map_err(Error::ParseInvitationId))
            .transpose()?,
        // If no token is present, we continue without an invitation id
        Err(auth::Error::ParseRequestToken(token::Error::MissingAuthHeader)) => None,
        // Or fail for any other reason
        Err(err) => return Err(err.into()),
    };

    let new_user = NewUser::new(&req.email, &req.first_name, &req.last_name, &req.password)?;
    let user = new_user.create(&mut write).await?;

    write
        .ctx
        .email
        .registration_confirmation(&user, invitation_id)
        .await?;

    Ok(api::UserServiceCreateResponse {
        user: Some(user.into()),
    })
}

pub async fn get(
    req: api::UserServiceGetRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::UserServiceGetResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseId)?;
    read.auth_or_for(&meta, UserAdminPerm::Get, UserPerm::Get, user_id)
        .await?;

    let user = User::by_id(user_id, &mut read).await?;

    Ok(api::UserServiceGetResponse {
        user: Some(user.into()),
    })
}

pub async fn list(
    req: api::UserServiceListRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::UserServiceListResponse, Error> {
    let filter = req.into_filter()?;
    if let Some(org_id) = filter.org_id {
        read.auth_or_for(&meta, UserAdminPerm::Filter, UserPerm::Filter, org_id)
            .await?
    } else {
        read.auth(&meta, UserAdminPerm::Filter).await?
    };

    let (users, _) = filter.query(&mut read).await?;
    let users = users.into_iter().map(Into::into).collect();

    Ok(api::UserServiceListResponse { users })
}

pub async fn update(
    req: api::UserServiceUpdateRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceUpdateResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseId)?;
    write
        .auth_or_for(&meta, UserAdminPerm::Update, UserPerm::Update, user_id)
        .await?;

    let update = UpdateUser {
        id: user_id,
        first_name: req.first_name.as_deref(),
        last_name: req.last_name.as_deref(),
    };
    let user = update.apply(&mut write).await?;

    Ok(api::UserServiceUpdateResponse {
        user: Some(user.into()),
    })
}

pub async fn delete(
    req: api::UserServiceDeleteRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::UserServiceDeleteResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseId)?;
    write.auth_for(&meta, UserPerm::Delete, user_id).await?;

    User::delete(user_id, &mut write).await?;

    Ok(api::UserServiceDeleteResponse {})
}

async fn get_settings(
    req: api::UserServiceGetSettingsRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::UserServiceGetSettingsResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    read.auth_or_for(
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
        .map(|s| (s.key.into(), s.value))
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
        .auth_or_for(
            &meta,
            UserSettingsAdminPerm::Update,
            UserSettingsPerm::Update,
            user_id,
        )
        .await?;

    let user = User::by_id(user_id, &mut write).await?;
    let setting = NewUserSetting::new(user.id, req.key, &req.value)
        .create_or_update(&mut write)
        .await?;

    Ok(api::UserServiceUpdateSettingsResponse {
        key: setting.key.into(),
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
        .auth_or_for(
            &meta,
            UserSettingsAdminPerm::Delete,
            UserSettingsPerm::Delete,
            user_id,
        )
        .await?;

    let user = User::by_id(user_id, &mut write).await?;
    UserSetting::delete(user.id, &req.key.into(), &mut write).await?;

    Ok(api::UserServiceDeleteSettingsResponse {})
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
                    id: search.user_id.map(|name| name.trim().to_lowercase()),
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
