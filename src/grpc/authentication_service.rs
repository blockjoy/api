use crate::auth::{
    FindableById, JwtToken, PwdResetToken, RegistrationConfirmationToken, TokenRole, TokenType,
    UserAuthToken, UserRefreshToken,
};
use crate::grpc::blockjoy_ui::authentication_service_server::AuthenticationService;
use crate::grpc::blockjoy_ui::{
    ApiToken, ConfirmRegistrationRequest, ConfirmRegistrationResponse, LoginUserRequest,
    LoginUserResponse, RefreshTokenRequest, RefreshTokenResponse, UpdateUiPasswordRequest,
    UpdateUiPasswordResponse,
};
use crate::grpc::helpers::required;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::mail::MailClient;
use crate::models;
use crate::models::User;
use tonic::{Request, Response, Status};

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
        let mut tx = self.db.begin().await?;
        let user = User::login(inner.clone(), &mut tx)
            .await
            .map_err(|e| Status::unauthenticated(e.to_string()))?;

        // On login, the refresh token gets renewed anyhow
        // @see https://app.shortcut.com/blockjoy/story/609/ability-to-login-after-refresh-token-has-expired
        tracing::debug!("Renewing user refresh token");
        let refresh_token = UserRefreshToken::create(user.id).encode()?;
        User::refresh(user.id, refresh_token.clone(), &mut tx).await?;
        tx.commit().await?;

        let auth_token =
            UserAuthToken::create_token_for::<User>(&user, TokenType::UserAuth, TokenRole::User)?;

        let response = LoginUserResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta)),
            token: Some(ApiToken {
                value: auth_token.to_base64()?,
            }),
        };

        Ok(response_with_refresh_token(refresh_token, response)?)
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
        let mut tx = self.db.begin().await?;
        let user = User::confirm(user_id, &mut tx).await?;
        let auth_token =
            UserAuthToken::create_token_for::<User>(&user, TokenType::UserAuth, TokenRole::User)?
                .encode()?;
        let refresh_token = UserRefreshToken::create_token_for::<User>(
            &user,
            TokenType::UserAuth,
            TokenRole::User,
        )?
        .encode()?;

        User::refresh(user.id, refresh_token.clone(), &mut tx).await?;
        tx.commit().await?;

        let response = ConfirmRegistrationResponse {
            meta: Some(ResponseMeta::from_meta(request.into_inner().meta)),
            token: Some(ApiToken { value: auth_token }),
        };

        Ok(response_with_refresh_token(refresh_token, response)?)
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
        let mut tx = self.db.begin().await?;
        let user = User::find_by_email(&request.email, &mut tx).await;
        if let Ok(user) = user {
            let _ = user.email_reset_password(&mut tx).await;
        }
        tx.commit().await?;

        let meta = ResponseMeta::new(String::from(""));
        let response = ResetPasswordResponse { meta: Some(meta) };

        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    async fn update_password(
        &self,
        request: Request<UpdatePasswordRequest>,
    ) -> Result<Response<UpdatePasswordResponse>, Status> {
        let token = request
            .extensions()
            .get::<PwdResetToken>()
            .ok_or_else(|| Status::unauthenticated("Invalid reset token"))?;
        let refresh_token = get_refresh_token(&request);
        let mut tx = self.db.begin().await?;
        let user_id = token.try_get_user(*token.id(), &mut tx).await?.id;
        let request = request.into_inner();
        let cur_user = User::find_by_id(user_id, &mut tx)
            .await?
            .update_password(&request.password, &mut tx)
            .await?;
        tx.commit().await?;
        let meta = ResponseMeta::from_meta(request.meta);
        let auth_token =
            UserAuthToken::create_token_for(&cur_user, TokenType::UserAuth, TokenRole::User)?;
        let response = UpdatePasswordResponse {
            meta: Some(meta),
            token: Some(ApiToken {
                value: auth_token.to_base64()?,
            }),
        };

        // Send notification mail
        MailClient::new().update_password(&cur_user).await?;

        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    async fn update_ui_password(
        &self,
        request: Request<UpdateUiPasswordRequest>,
    ) -> Result<Response<UpdateUiPasswordResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?;
        let mut tx = self.db.begin().await?;
        let user = token.try_get_user(*token.id(), &mut tx).await?;
        let encoded = token
            .encode()
            .map_err(|e| Status::internal(format!("Token encode error {e:?}")))?;
        let inner = request.into_inner();

        let resp = match user.verify_password(&inner.old_pwd) {
            Ok(_) => {
                if inner.new_pwd == inner.new_pwd_confirmation {
                    user.update_password(&inner.new_pwd, &mut tx).await?;

                    let response = UpdateUiPasswordResponse {
                        meta: None,
                        token: Some(ApiToken { value: encoded }),
                    };

                    // Send notification mail
                    MailClient::new().update_password(&user).await?;

                    Ok(response_with_refresh_token(refresh_token, response)?)
                } else {
                    Err(Status::invalid_argument(
                        "Password and password confirmation don't match",
                    ))
                }
            }
            Err(e) => Err(Status::from(e)),
        };
        tx.commit().await?;
        resp
    }
}
