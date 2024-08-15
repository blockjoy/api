use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::{error, warn};

use crate::auth::claims::{Claims, Expirable, Granted};
use crate::auth::rbac::{AuthAdminPerm, AuthPerm, GrpcRole};
use crate::auth::resource::{Resource, ResourceId};
use crate::auth::token::refresh::Refresh;
use crate::auth::token::RequestToken;
use crate::auth::Authorize;
use crate::database::{Transaction, WriteConn};
use crate::model::{Host, Node, Org, User};

use super::api::auth_service_server::AuthService;
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
    /// Failed to send email: {0}
    Email(#[from] crate::email::Error),
    /// Host auth error: {0}
    Host(#[from] crate::model::host::Error),
    /// JWT token failure: {0}
    Jwt(#[from] crate::auth::token::jwt::Error),
    /// Node auth error: {0}
    Node(#[from] crate::model::node::Error),
    /// Not Bearer Token.
    NotBearer,
    /// No Refresh token in cookie or request body.
    NoRefresh,
    /// Org auth error: {0}
    Org(#[from] crate::model::org::Error),
    /// Failed to parse RequestToken: {0}
    ParseToken(crate::auth::token::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse UserId: {0}
    ParseUserId(uuid::Error),
    /// User RBAC error: {0}
    Rbac(#[from] crate::model::rbac::Error),
    /// Refresh token failure: {0}
    Refresh(#[from] crate::auth::token::refresh::Error),
    /// Refresh token doesn't match JWT Resource.
    RefreshResource,
    /// User auth error: {0}
    User(#[from] crate::model::user::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            ClaimsNotUser | Jwt(_) | ParseToken(_) | RefreshResource => {
                Status::permission_denied("Access denied.")
            }
            Diesel(_) | Email(_) => Status::internal("Internal error."),
            NotBearer => Status::unauthenticated("Not bearer."),
            NoRefresh => Status::invalid_argument("No refresh token."),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseUserId(_) => Status::invalid_argument("user_id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Host(err) => err.into(),
            Node(err) => err.into(),
            Org(err) => err.into(),
            Rbac(err) => err.into(),
            Refresh(err) => err.into(),
            User(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl AuthService for Grpc {
    async fn login(
        &self,
        req: Request<api::AuthServiceLoginRequest>,
    ) -> Result<Response<api::AuthServiceLoginResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| login(req, meta, write).scope_boxed())
            .await
    }

    async fn confirm(
        &self,
        req: Request<api::AuthServiceConfirmRequest>,
    ) -> Result<Response<api::AuthServiceConfirmResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| confirm(req, meta, write).scope_boxed())
            .await
    }

    async fn refresh(
        &self,
        req: Request<api::AuthServiceRefreshRequest>,
    ) -> Result<Response<api::AuthServiceRefreshResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| refresh(req, meta, write).scope_boxed())
            .await
    }

    async fn reset_password(
        &self,
        req: Request<api::AuthServiceResetPasswordRequest>,
    ) -> Result<Response<api::AuthServiceResetPasswordResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| reset_password(req, meta, write).scope_boxed())
            .await
    }

    async fn update_password(
        &self,
        req: Request<api::AuthServiceUpdatePasswordRequest>,
    ) -> Result<Response<api::AuthServiceUpdatePasswordResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_password(req, meta, write).scope_boxed())
            .await
    }

    async fn update_ui_password(
        &self,
        req: Request<api::AuthServiceUpdateUiPasswordRequest>,
    ) -> Result<Response<api::AuthServiceUpdateUiPasswordResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_ui_password(req, meta, write).scope_boxed())
            .await
    }

    async fn list_permissions(
        &self,
        req: Request<api::AuthServiceListPermissionsRequest>,
    ) -> Result<Response<api::AuthServiceListPermissionsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| list_permissions(req, meta, write).scope_boxed())
            .await
    }
}

async fn login(
    req: api::AuthServiceLoginRequest,
    _: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::AuthServiceLoginResponse, Error> {
    // No auth claims are required as the password is checked instead.
    let user = User::login(&req.email, &req.password, &mut write).await?;

    let expires = write.ctx.config.token.expire.token;
    let claims = Claims::from_now(expires, user.id, GrpcRole::Login);

    let expires = write.ctx.config.token.expire.refresh_user;
    let refresh = Refresh::from_now(expires, user.id);
    let cookie = write.ctx.auth.cipher.refresh.cookie(&refresh)?;
    write.meta("set-cookie", cookie.header()?);

    Ok(api::AuthServiceLoginResponse {
        token: write.ctx.auth.cipher.jwt.encode(&claims)?.into(),
        refresh: write.ctx.auth.cipher.refresh.encode(&refresh)?.into(),
    })
}

async fn confirm(
    _: api::AuthServiceConfirmRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::AuthServiceConfirmResponse, Error> {
    let authz = write.auth_all(&meta, AuthPerm::Confirm).await?;
    let user_id = authz.resource().user().ok_or(Error::ClaimsNotUser)?;

    let expire = &write.ctx.config.token.expire;
    let claims = Claims::from_now(expire.token, user_id, GrpcRole::Login);

    User::confirm(user_id, &mut write).await?;

    let refresh = Refresh::from_now(expire.refresh_user, user_id);
    let cookie = write.ctx.auth.cipher.refresh.cookie(&refresh)?;
    write.meta("set-cookie", cookie.header()?);

    Ok(api::AuthServiceConfirmResponse {
        token: write.ctx.auth.cipher.jwt.encode(&claims)?.into(),
        refresh: write.ctx.auth.cipher.refresh.encode(&refresh)?.into(),
    })
}

async fn refresh(
    req: api::AuthServiceRefreshRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::AuthServiceRefreshResponse, Error> {
    let claims = match req.token.parse().map_err(Error::ParseToken)? {
        RequestToken::Bearer(token) => write.ctx.auth.cipher.jwt.decode_expired(&token)?,
        RequestToken::ApiKey(_) => Err(Error::NotBearer)?,
    };

    let refresh = if let Some(refresh) = req.refresh {
        write.ctx.auth.cipher.refresh.decode(&refresh.into())?
    } else {
        let fallback = write.ctx.auth.maybe_refresh(&meta)?;
        fallback.ok_or(Error::NoRefresh)?
    };

    // Verify that the resource still exists.
    let resource = claims.resource();
    let resource_id: ResourceId = match resource {
        Resource::User(id) => User::by_id(id, &mut write).await.map(|_| id.into())?,
        Resource::Org(id) => Org::by_id(id, &mut write).await.map(|_| id.into())?,
        Resource::Host(id) => Host::by_id(id, &mut write).await.map(|_| id.into())?,
        Resource::Node(id) => Node::by_id(id, &mut write).await.map(|_| id.into())?,
    };

    // Check that the claims and the refresh token refer to the same user
    if resource_id != refresh.resource_id() {
        return Err(Error::RefreshResource);
    }

    let expirable = Expirable::from_now(write.ctx.config.token.expire.token);
    let new_claims = if let Some(data) = claims.data {
        Claims::new(resource, expirable, claims.access).with_data(data)
    } else {
        Claims::new(resource, expirable, claims.access)
    };
    let token = write.ctx.auth.cipher.jwt.encode(&new_claims)?;

    let expires = refresh.expirable().duration();
    let refresh = Refresh::from_now(expires, resource);

    let encoded = write.ctx.auth.cipher.refresh.encode(&refresh)?;
    let cookie = write.ctx.auth.cipher.refresh.cookie(&refresh)?;
    write.meta("set-cookie", cookie.header()?);

    Ok(api::AuthServiceRefreshResponse {
        token: token.into(),
        refresh: encoded.into(),
    })
}

/// This endpoint triggers the sending of the reset-password email. The actual resetting is
/// then done through the `update` function.
async fn reset_password(
    req: api::AuthServiceResetPasswordRequest,
    _: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::AuthServiceResetPasswordResponse, Error> {
    // We are going to query the user and send them an email, but when something goes wrong we
    // are not going to return an error. This hides whether or not a user is registered with
    // us to the caller of the api, because this info may be sensitive and this endpoint is not
    // protected by any authentication.
    match User::by_email(&req.email, &mut write).await {
        Ok(user) => {
            if let Err(err) = write.ctx.email.reset_password(&user).await {
                warn!("Failed to reset password: {err}");
            }
        }
        Err(err) => warn!("Failed to find user to reset password: {err}"),
    }

    Ok(api::AuthServiceResetPasswordResponse {})
}

async fn update_password(
    req: api::AuthServiceUpdatePasswordRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::AuthServiceUpdatePasswordResponse, Error> {
    let authz = write.auth_all(&meta, AuthPerm::UpdatePassword).await?;
    let user_id = authz.resource().user().ok_or(Error::ClaimsNotUser)?;

    let user = User::by_id(user_id, &mut write).await?;
    user.update_password(&req.password, &mut write).await?;

    write.ctx.email.update_password(&user).await?;

    Ok(api::AuthServiceUpdatePasswordResponse {})
}

async fn update_ui_password(
    req: api::AuthServiceUpdateUiPasswordRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::AuthServiceUpdateUiPasswordResponse, Error> {
    let user_id = req.user_id.parse().map_err(Error::ParseUserId)?;
    write
        .auth(&meta, AuthPerm::UpdateUiPassword, user_id)
        .await?;

    let user = User::by_id(user_id, &mut write).await?;
    user.verify_password(&req.old_password)?;
    user.update_password(&req.new_password, &mut write).await?;

    write.ctx.email.update_password(&user).await?;

    Ok(api::AuthServiceUpdateUiPasswordResponse {})
}

async fn list_permissions(
    req: api::AuthServiceListPermissionsRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::AuthServiceListPermissionsResponse, Error> {
    let user_id = req.user_id.parse().map_err(Error::ParseUserId)?;
    let org_id = req.org_id.parse().map_err(Error::ParseOrgId)?;

    let (authz, ensure_member) = match write.auth_all(&meta, AuthAdminPerm::ListPermissions).await {
        Ok(authz) => (authz, false),
        Err(crate::auth::Error::Claims(_)) => {
            let authz = write.auth_all(&meta, AuthPerm::ListPermissions).await?;
            (authz, true)
        }
        Err(err) => return Err(err.into()),
    };

    let granted = Granted::for_org(user_id, org_id, ensure_member, &mut write).await?;
    let granted = if req.include_token.unwrap_or_default() {
        Granted::from_access(&authz.claims.access, Some(granted), &mut write).await?
    } else {
        granted
    };

    let mut permissions: Vec<_> = granted.iter().map(ToString::to_string).collect();
    permissions.sort();

    Ok(api::AuthServiceListPermissionsResponse { permissions })
}
