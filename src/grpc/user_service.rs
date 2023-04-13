use super::api::{self, users_server};
use super::convert;
use super::helpers::{required, try_get_token};
use crate::auth::{JwtToken, UserAuthToken};
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::mail::MailClient;
use crate::models;
use crate::models::User;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};

impl api::CreateUserRequest {
    fn as_new(&self) -> crate::Result<models::NewUser> {
        models::NewUser::new(
            &self.email,
            &self.first_name,
            &self.last_name,
            &self.password,
        )
    }
}

impl api::UpdateUserRequest {
    pub fn as_update(&self) -> crate::Result<models::UpdateUser<'_>> {
        Ok(models::UpdateUser {
            id: self.id.parse()?,
            first_name: self.first_name.as_deref(),
            last_name: self.last_name.as_deref(),

            // For obvious reasons, users are not allowed to update these fields
            fee_bps: None,
            staking_quota: None,
            refresh: None,
        })
    }
}

impl api::User {
    pub fn from_model(model: models::User) -> crate::Result<Self> {
        let user = Self {
            id: model.id.to_string(),
            email: model.email,
            first_name: model.first_name,
            last_name: model.last_name,
            created_at: Some(convert::try_dt_to_ts(model.created_at)?),
            updated_at: None,
        };
        Ok(user)
    }
}

#[tonic::async_trait]
impl users_server::Users for super::GrpcImpl {
    async fn get(
        &self,
        request: Request<api::GetUserRequest>,
    ) -> super::Result<api::GetUserResponse> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let mut conn = self.conn().await?;
        let user = token.try_get_user(token.id, &mut conn).await?;
        let inner = request.into_inner();
        let response = api::GetUserResponse {
            user: Some(api::User::from_model(user)?),
        };

        response_with_refresh_token(refresh_token, response)
    }

    async fn create(
        &self,
        request: Request<api::CreateUserRequest>,
    ) -> super::Result<api::CreateUserResponse> {
        let inner = request.into_inner();
        if inner.password != inner.password_confirmation {
            return Err(Status::invalid_argument("Passwords don't match"));
        }
        let new_user = inner.as_new()?;
        let new_user = self.trx(|c| new_user.create(c).scope_boxed()).await?;
        let response = api::CreateUserResponse {};

        MailClient::new()
            .registration_confirmation(&new_user)
            .await?;

        Ok(Response::new(response))
    }

    async fn update(
        &self,
        request: Request<api::UpdateUserRequest>,
    ) -> super::Result<api::UpdateUserResponse> {
        let refresh_token = get_refresh_token(&request);
        let token = request
            .extensions()
            .get::<UserAuthToken>()
            .ok_or_else(required("auth token"))?
            .clone();
        self.trx(|c| {
            async move {
                let user_id = token.try_get_user(token.id, c).await?.id;
                let inner = request.into_inner();

                // Check if current user is the same as the one to be updated
                if user_id.to_string() != inner.id {
                    super::bail_unauthorized!("You are not allowed to update this user");
                }
                let user = inner.as_update()?.update(c).await?;
                let resp = api::UpdateUserResponse {
                    user: Some(api::User::from_model(user)?),
                };

                Ok(response_with_refresh_token(refresh_token, resp)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn delete(
        &self,
        request: Request<api::DeleteUserRequest>,
    ) -> Result<Response<()>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = request
            .extensions()
            .get::<UserAuthToken>()
            .ok_or_else(required("auth token"))?;
        self.trx(|c| {
            async move {
                let user_id = token.try_get_user(token.id, c).await?.id;
                User::delete(user_id, c).await
            }
            .scope_boxed()
        })
        .await?;

        response_with_refresh_token(refresh_token, ())
    }

    async fn upsert_configuration(
        &self,
        _request: Request<api::UpsertConfigurationRequest>,
    ) -> super::Result<api::UpsertConfigurationResponse> {
        Err(Status::unimplemented(""))
    }

    async fn get_configuration(
        &self,
        _request: Request<api::GetConfigurationRequest>,
    ) -> super::Result<api::GetConfigurationResponse> {
        Err(Status::unimplemented(""))
    }
}
