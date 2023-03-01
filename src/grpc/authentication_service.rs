use crate::auth::{
    FindableById, JwtToken, PwdResetToken, RegistrationConfirmationToken, TokenRole, TokenType,
    UserAuthToken, UserRefreshToken,
};
use crate::errors::ApiError;
use crate::grpc::blockjoy_ui::authentication_service_server::AuthenticationService;
use crate::grpc::blockjoy_ui::{
    ApiToken, ConfirmRegistrationRequest, ConfirmRegistrationResponse, LoginUserRequest,
    LoginUserResponse, RefreshTokenRequest, RefreshTokenResponse, SwitchOrgRequest,
    UpdateUiPasswordRequest, UpdateUiPasswordResponse,
};
use crate::grpc::helpers::required;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::mail::MailClient;
use crate::models;
use crate::models::{Org, User};
use diesel_async::scoped_futures::ScopedFutureExt;
use std::collections::HashMap;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::blockjoy_ui::{
    ResetPasswordRequest, ResetPasswordResponse, ResponseMeta, UpdatePasswordRequest,
    UpdatePasswordResponse,
};
use super::helpers::try_get_token;

pub struct AuthenticationServiceImpl {
    db: models::DbPool,
}

impl AuthenticationServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl AuthenticationService for AuthenticationServiceImpl {
    async fn login(
        &self,
        request: Request<LoginUserRequest>,
    ) -> Result<Response<LoginUserResponse>, Status> {
        let inner = request.into_inner();
        // User::login checks if user is confirmed before testing for valid login credentials
        let resp = self
            .db
            .trx(|c| {
                {
                    async move {
                        let user = User::login(inner.clone(), c)
                            .await
                            .map_err(|e| Status::unauthenticated(e.to_string()))?;

                        // On login, the refresh token gets renewed anyhow
                        // @see https://app.shortcut.com/blockjoy/story/609/ability-to-login-after-refresh-token-has-expired
                        tracing::debug!("Renewing user refresh token");
                        let refresh_token = UserRefreshToken::create(user.id).encode()?;
                        User::set_refresh(user.id, &refresh_token, c).await?;

                        // User personal org by default
                        let org = Org::find_personal_org(user.id, c).await?;
                        let org_user = Org::find_org_user(user.id, org.id, c).await?;
                        let mut token_data = HashMap::<String, String>::new();

                        token_data.insert("email".to_string(), user.email.clone());

                        let auth_token = UserAuthToken::create_token_for::<User>(
                            &user,
                            TokenType::UserAuth,
                            TokenRole::User,
                            Some(token_data),
                        )?;
                        let auth_token = auth_token.set_org_user(&org_user);

                        let response = LoginUserResponse {
                            meta: Some(ResponseMeta::from_meta(inner.meta, None)),
                            token: Some(ApiToken {
                                value: auth_token.to_base64()?,
                            }),
                        };
                        Ok(response_with_refresh_token(Some(refresh_token), response)?)
                    }
                    .scope_boxed()
                }
                .scope_boxed()
            })
            .await?;
        Ok(resp)
    }

    async fn confirm(
        &self,
        request: Request<ConfirmRegistrationRequest>,
    ) -> Result<Response<ConfirmRegistrationResponse>, Status> {
        let token = request
            .extensions()
            .get::<RegistrationConfirmationToken>()
            .ok_or_else(required("Registration confirmation token extension"))?;
        let user_id = token.get_id();
        let resp = self
            .db
            .trx(|c| {
                async move {
                    let user = User::confirm(user_id, c).await?;
                    let auth_token = UserAuthToken::create_token_for::<User>(
                        &user,
                        TokenType::UserAuth,
                        TokenRole::User,
                        None,
                    )?
                    .encode()?;
                    let refresh_token = UserRefreshToken::create_token_for::<User>(
                        &user,
                        TokenType::UserAuth,
                        TokenRole::User,
                        None,
                    )?
                    .encode()?;

                    User::set_refresh(user.id, &refresh_token, c).await?;

                    let response = ConfirmRegistrationResponse {
                        meta: Some(ResponseMeta::from_meta(request.into_inner().meta, None)),
                        token: Some(ApiToken { value: auth_token }),
                    };

                    Ok(response_with_refresh_token(Some(refresh_token), response)?)
                }
                .scope_boxed()
            })
            .await?;
        Ok(resp)
    }

    async fn refresh(
        &self,
        _request: Request<RefreshTokenRequest>,
    ) -> Result<Response<RefreshTokenResponse>, Status> {
        Err(Status::unimplemented("Not necessary anymore"))
    }

    /// This endpoint triggers the sending of the reset-password email. The actual resetting is
    /// then done through the `update` function.
    async fn reset_password(
        &self,
        request: Request<ResetPasswordRequest>,
    ) -> Result<Response<ResetPasswordResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let request = request.into_inner();
        // We are going to query the user and send them an email, but when something goes wrong we
        // are not going to return an error. This hides whether or not a user is registered with
        // us to the caller of the api, because this info may be sensitive and this endpoint is not
        // protected by any authentication.
        let response = self
            .db
            .trx(|c| {
                async move {
                    let user = User::find_by_email(&request.email, c).await;
                    if let Ok(user) = user {
                        let _ = user.email_reset_password(c).await;
                    }

                    let meta = ResponseMeta::new(String::from(""), None);
                    let response = ResetPasswordResponse { meta: Some(meta) };
                    Ok(response)
                }
                .scope_boxed()
            })
            .await?;
        response_with_refresh_token(refresh_token, response)
    }

    async fn update_password(
        &self,
        request: Request<UpdatePasswordRequest>,
    ) -> Result<Response<UpdatePasswordResponse>, Status> {
        let (refresh_token, response) = self
            .db
            .trx(|c| {
                async move {
                    let token = request
                        .extensions()
                        .get::<PwdResetToken>()
                        .ok_or_else(|| Status::unauthenticated("Invalid reset token"))?;
                    let refresh_token = get_refresh_token(&request);
                    let user_id = token.try_get_user(token.id, c).await?.id;
                    let request = request.into_inner();
                    let cur_user = User::find_by_id(user_id, c)
                        .await?
                        .update_password(&request.password, c)
                        .await?;
                    let meta = ResponseMeta::from_meta(request.meta, None);
                    let auth_token = UserAuthToken::create_token_for(
                        &cur_user,
                        TokenType::UserAuth,
                        TokenRole::User,
                        None,
                    )?;
                    let response = UpdatePasswordResponse {
                        meta: Some(meta),
                        token: Some(ApiToken {
                            value: auth_token.to_base64()?,
                        }),
                    };

                    // Send notification mail
                    MailClient::new().update_password(&cur_user).await?;
                    Ok((refresh_token, response))
                }
                .scope_boxed()
            })
            .await?;

        response_with_refresh_token(refresh_token, response)
    }

    async fn update_ui_password(
        &self,
        request: Request<UpdateUiPasswordRequest>,
    ) -> Result<Response<UpdateUiPasswordResponse>, Status> {
        let (refresh_token, response) = self
            .db
            .trx(|c| {
                async move {
                    let refresh_token = get_refresh_token(&request);
                    let token = try_get_token::<_, UserAuthToken>(&request)?;
                    let user = token.try_get_user(token.id, c).await?;
                    let encoded = token
                        .encode()
                        .map_err(|e| Status::internal(format!("Token encode error {e:?}")))?;
                    let inner = request.into_inner();

                    user.verify_password(&inner.old_pwd)?;
                    if inner.new_pwd != inner.new_pwd_confirmation {
                        return Err(Status::invalid_argument(
                            "Password and password confirmation don't match",
                        )
                        .into());
                    }

                    user.update_password(&inner.new_pwd, c).await?;

                    let response = UpdateUiPasswordResponse {
                        meta: None,
                        token: Some(ApiToken { value: encoded }),
                    };

                    // Send notification mail
                    MailClient::new().update_password(&user).await?;

                    Ok((refresh_token, response))
                }
                .scope_boxed()
            })
            .await?;
        response_with_refresh_token(refresh_token, response)
    }

    async fn switch_organization(
        &self,
        request: Request<SwitchOrgRequest>,
    ) -> Result<Response<LoginUserResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let user_id = token.get_id();
        let inner = request.into_inner();
        let mut conn = self.db.conn().await?;
        let org_user = Org::find_org_user(
            user_id,
            Uuid::parse_str(inner.org_id.as_str()).map_err(ApiError::from)?,
            &mut conn,
        )
        .await?;
        let auth_token =
            UserAuthToken::create_token_for(&org_user, TokenType::UserAuth, TokenRole::User, None)?;
        let auth_token = auth_token.set_org_user(&org_user);

        let response = LoginUserResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token.try_into()?))),
            token: Some(ApiToken {
                value: auth_token.to_base64()?,
            }),
        };

        response_with_refresh_token(refresh_token, response)
    }
}
