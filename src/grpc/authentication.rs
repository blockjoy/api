use super::api::{self, auth_service_server};
use super::helpers::{required, try_get_token};
use super::{get_refresh_token, response_with_refresh_token};
use crate::auth::{
    FindableById, JwtToken, PwdResetToken, RegistrationConfirmationToken, TokenRole, TokenType,
    UserAuthToken, UserRefreshToken,
};
use crate::mail::MailClient;
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use std::collections::HashMap;
use tonic::{Request, Status};

#[tonic::async_trait]
impl auth_service_server::AuthService for super::GrpcImpl {
    async fn login(
        &self,
        request: Request<api::AuthServiceLoginRequest>,
    ) -> super::Result<api::AuthServiceLoginResponse> {
        let inner = request.into_inner();
        // User::login checks if user is confirmed before testing for valid login credentials
        self.trx(|c| {
            async move {
                let user = models::User::login(&inner.email, &inner.password, c)
                    .await
                    .map_err(|e| Status::unauthenticated(e.to_string()))?;

                // On login, the refresh token gets renewed anyhow
                // @see https://app.shortcut.com/blockjoy/story/609/ability-to-login-after-refresh-token-has-expired
                tracing::debug!("Renewing user refresh token");
                let refresh_token = UserRefreshToken::create(user.id).encode()?;
                models::User::set_refresh(user.id, &refresh_token, c).await?;

                // User personal org by default
                let org = models::Org::find_personal_org(user.id, c).await?;
                let org_user = models::Org::find_org_user(user.id, org.id, c).await?;
                let mut token_data = HashMap::<String, String>::new();

                token_data.insert("email".to_string(), user.email.clone());

                let auth_token = UserAuthToken::create_token_for::<models::User>(
                    &user,
                    TokenType::UserAuth,
                    TokenRole::User,
                    Some(token_data),
                )?;
                let auth_token = auth_token.set_org_user(&org_user);

                let response = api::AuthServiceLoginResponse {
                    token: auth_token.to_base64()?,
                };
                Ok(response_with_refresh_token(Some(refresh_token), response)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn confirm(
        &self,
        request: Request<api::AuthServiceConfirmRequest>,
    ) -> super::Result<api::AuthServiceConfirmResponse> {
        let token = request
            .extensions()
            .get::<RegistrationConfirmationToken>()
            .ok_or_else(required("Registration confirmation token extension"))?;
        let user_id = token.get_id();
        self.trx(|c| {
            async move {
                let user = models::User::confirm(user_id, c).await?;
                let token = UserAuthToken::create_token_for::<models::User>(
                    &user,
                    TokenType::UserAuth,
                    TokenRole::User,
                    None,
                )?
                .encode()?;
                let refresh_token = UserRefreshToken::create_token_for::<models::User>(
                    &user,
                    TokenType::UserAuth,
                    TokenRole::User,
                    None,
                )?
                .encode()?;

                models::User::set_refresh(user.id, &refresh_token, c).await?;

                let response = api::AuthServiceConfirmResponse { token };

                Ok(response_with_refresh_token(Some(refresh_token), response)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn refresh(
        &self,
        _request: Request<api::AuthServiceRefreshRequest>,
    ) -> super::Result<api::AuthServiceRefreshResponse> {
        Err(Status::unimplemented("Not necessary anymore"))
    }

    /// This endpoint triggers the sending of the reset-password email. The actual resetting is
    /// then done through the `update` function.
    async fn reset_password(
        &self,
        request: Request<api::AuthServiceResetPasswordRequest>,
    ) -> super::Result<api::AuthServiceResetPasswordResponse> {
        let refresh_token = get_refresh_token(&request);
        let request = request.into_inner();
        // We are going to query the user and send them an email, but when something goes wrong we
        // are not going to return an error. This hides whether or not a user is registered with
        // us to the caller of the api, because this info may be sensitive and this endpoint is not
        // protected by any authentication.
        self.trx(|c| {
            async move {
                let user = models::User::find_by_email(&request.email, c).await;
                if let Ok(user) = user {
                    let _ = user.email_reset_password(c).await;
                }

                let response = api::AuthServiceResetPasswordResponse {};
                Ok(response_with_refresh_token(refresh_token, response)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn update_password(
        &self,
        request: Request<api::AuthServiceUpdatePasswordRequest>,
    ) -> super::Result<api::AuthServiceUpdatePasswordResponse> {
        self.trx(|c| {
            async move {
                let token = request
                    .extensions()
                    .get::<PwdResetToken>()
                    .ok_or_else(|| Status::unauthenticated("Invalid reset token"))?;
                let refresh_token = get_refresh_token(&request);
                let user_id = token.try_get_user(token.id, c).await?.id;
                let request = request.into_inner();
                let cur_user = models::User::find_by_id(user_id, c)
                    .await?
                    .update_password(&request.password, c)
                    .await?;
                let auth_token = UserAuthToken::create_token_for(
                    &cur_user,
                    TokenType::UserAuth,
                    TokenRole::User,
                    None,
                )?;
                let response = api::AuthServiceUpdatePasswordResponse {
                    token: auth_token.to_base64()?,
                };

                // Send notification mail
                MailClient::new().update_password(&cur_user).await?;
                Ok(response_with_refresh_token(refresh_token, response)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn update_ui_password(
        &self,
        request: Request<api::AuthServiceUpdateUiPasswordRequest>,
    ) -> super::Result<api::AuthServiceUpdateUiPasswordResponse> {
        self.trx(|c| {
            async move {
                let refresh_token = get_refresh_token(&request);
                let token = try_get_token::<_, UserAuthToken>(&request)?;
                let user = token.try_get_user(token.id, c).await?;
                let encoded = token
                    .encode()
                    .map_err(|e| Status::internal(format!("Token encode error {e:?}")))?;
                let inner = request.into_inner();

                user.verify_password(&inner.old_password)?;
                user.update_password(&inner.new_password, c).await?;

                let response = api::AuthServiceUpdateUiPasswordResponse { token: encoded };

                // Send notification mail
                MailClient::new().update_password(&user).await?;
                Ok(response_with_refresh_token(refresh_token, response)?)
            }
            .scope_boxed()
        })
        .await
    }
}
