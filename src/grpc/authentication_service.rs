use crate::auth::{FindableById, TokenIdentifyable};
use crate::grpc::blockjoy_ui::authentication_service_server::AuthenticationService;
use crate::grpc::blockjoy_ui::{
    ApiToken, LoginUserRequest, LoginUserResponse, RefreshTokenRequest, RefreshTokenResponse,
};
use crate::grpc::helpers::success_response_meta;
use crate::models::{self, Token, User};
use crate::server::DbPool;
use tonic::{Request, Response, Status};

use super::blockjoy_ui::response_meta::Status::Success;
use super::blockjoy_ui::{
    ResetPasswordRequest, ResetPasswordResponse, ResponseMeta, UpdatePasswordRequest,
    UpdatePasswordResponse,
};

pub struct AuthenticationServiceImpl {
    db: DbPool,
}

impl AuthenticationServiceImpl {
    pub fn new(db: DbPool) -> Self {
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
        let user = User::login(inner.clone(), &self.db).await?;
        let db_token = user.get_token(&self.db).await?;
        let token = ApiToken {
            value: db_token.token,
        };
        let meta = success_response_meta(inner.meta.unwrap().id);
        let response = LoginUserResponse {
            meta: Some(meta),
            token: Some(token),
        };

        Ok(Response::new(response))
    }

    async fn refresh(
        &self,
        request: Request<RefreshTokenRequest>,
    ) -> Result<Response<RefreshTokenResponse>, Status> {
        let db_token = request.extensions().get::<Token>().unwrap().token.clone();
        let inner = request.into_inner();
        let old_token = inner.meta.clone().unwrap().token.unwrap().value;
        let request_id = inner.meta.unwrap().id;

        if db_token == old_token {
            let new_token = ApiToken {
                value: Token::refresh(db_token, &self.db).await?.token,
            };

            let meta = success_response_meta(request_id);
            let response = RefreshTokenResponse {
                meta: Some(meta),
                token: Some(new_token),
            };

            Ok(Response::new(response))
        } else {
            Err(Status::permission_denied("Not allowed to modify token"))
        }
    }

    /// This endpoint triggers the sending of the reset-password email. The actual resetting is
    /// then done through the `update` function.
    async fn reset_password(
        &self,
        request: Request<ResetPasswordRequest>,
    ) -> Result<Response<ResetPasswordResponse>, Status> {
        let request = request.into_inner();
        // We are going to query the user and send them an email, but when something goes wrong we
        // are not going to return an error. This hides whether or not a user is registered with
        // us to the caller of the api, because this info may be sensitive and this endpoint is not
        // protected by any authentication.
        let user = models::User::find_by_email(&request.email, &self.db).await;
        if let Ok(user) = user {
            let _ = user.email_reset_password(&self.db).await;
        }

        let meta = ResponseMeta {
            status: Success.into(),
            origin_request_id: None,
            messages: vec![],
            pagination: None,
        };
        let response = ResetPasswordResponse { meta: Some(meta) };
        Ok(Response::new(response))
    }

    async fn update_password(
        &self,
        request: tonic::Request<UpdatePasswordRequest>,
    ) -> Result<tonic::Response<UpdatePasswordResponse>, tonic::Status> {
        let db_token = request.extensions().get::<Token>().unwrap();
        let user_id = db_token.user_id.unwrap();
        let cur_user = models::User::find_by_id(user_id, &self.db).await?;
        let request = request.into_inner();
        let _cur_user = cur_user
            .update_password(&request.password, &self.db)
            .await?;
        let meta = success_response_meta(request.meta.unwrap().id);
        let response = UpdatePasswordResponse { meta: Some(meta) };
        Ok(Response::new(response))
    }
}
