use super::convert;
use super::helpers::{required, try_get_token};
use crate::auth::{JwtToken, UserAuthToken};
use crate::grpc::blockjoy_ui::user_service_server::UserService;
use crate::grpc::blockjoy_ui::{
    self, CreateUserRequest, CreateUserResponse, DeleteUserRequest, GetConfigurationRequest,
    GetConfigurationResponse, GetUserRequest, GetUserResponse, ResponseMeta, UpdateUserRequest,
    UpdateUserResponse, UpsertConfigurationRequest, UpsertConfigurationResponse,
};
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::mail::MailClient;
use crate::models;
use crate::models::User;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};

pub struct UserServiceImpl {
    db: models::DbPool,
}

impl UserServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

impl blockjoy_ui::User {
    pub fn as_update(&self) -> crate::Result<models::UpdateUser<'_>> {
        Ok(models::UpdateUser {
            id: self.id.as_ref().ok_or_else(required("user.id"))?.parse()?,
            first_name: self.first_name.as_deref(),
            last_name: self.last_name.as_deref(),

            // For obvious reasons, users are not allowed to update these fields
            fee_bps: None,
            staking_quota: None,
            refresh: None,
        })
    }

    pub fn from_model(model: models::User) -> crate::Result<Self> {
        let user = Self {
            id: Some(model.id.to_string()),
            email: Some(model.email),
            first_name: Some(model.first_name),
            last_name: Some(model.last_name),
            created_at: Some(convert::try_dt_to_ts(model.created_at)?),
            updated_at: None,
        };
        Ok(user)
    }
}

#[tonic::async_trait]
impl UserService for UserServiceImpl {
    async fn get(
        &self,
        request: Request<GetUserRequest>,
    ) -> Result<Response<GetUserResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let mut conn = self.db.conn().await?;
        let user = token.try_get_user(token.id, &mut conn).await?;
        let inner = request.into_inner();
        let response = GetUserResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token.try_into()?))),
            user: Some(blockjoy_ui::User::from_model(user)?),
        };

        response_with_refresh_token(refresh_token, response)
    }

    async fn create(
        &self,
        request: Request<CreateUserRequest>,
    ) -> Result<Response<CreateUserResponse>, Status> {
        let inner = request.into_inner();
        let user = inner.user.ok_or_else(required("user"))?;
        if inner.password != inner.password_confirmation {
            return Err(Status::invalid_argument("Passwords don't match"));
        }
        let new_user = models::NewUser::new(
            user.email.as_deref().ok_or_else(required("email"))?,
            user.first_name
                .as_deref()
                .ok_or_else(required("first_name"))?,
            user.last_name
                .as_deref()
                .ok_or_else(required("last_name"))?,
            &inner.password,
        )?;
        let new_user = self.db.trx(|c| new_user.create(c).scope_boxed()).await?;
        let meta = ResponseMeta::from_meta(inner.meta, None).with_message(new_user.id);
        let response = CreateUserResponse { meta: Some(meta) };

        MailClient::new()
            .registration_confirmation(&new_user)
            .await?;

        Ok(Response::new(response))
    }

    async fn update(
        &self,
        request: Request<UpdateUserRequest>,
    ) -> Result<Response<UpdateUserResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = request
            .extensions()
            .get::<UserAuthToken>()
            .ok_or_else(required("auth token"))?
            .clone();
        let response = self
            .db
            .trx(|c| {
                async move {
                    let user_id = token.try_get_user(token.id, c).await?.id;
                    let inner = request.into_inner();
                    let user = inner.user.ok_or_else(required("user"))?;

                    // Check if current user is the same as the one to be updated
                    if user_id.to_string() != user.id() {
                        return Err(Status::permission_denied(
                            "You are not allowed to update this user",
                        )
                        .into());
                    }
                    let user = user.as_update()?.update(c).await?;
                    let response_meta =
                        ResponseMeta::from_meta(inner.meta, Some(token.try_into()?));
                    Ok(UpdateUserResponse {
                        meta: Some(response_meta),
                        user: Some(blockjoy_ui::User::from_model(user)?),
                    })
                }
                .scope_boxed()
            })
            .await?;

        response_with_refresh_token(refresh_token, response)
    }

    async fn delete(&self, request: Request<DeleteUserRequest>) -> Result<Response<()>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = request
            .extensions()
            .get::<UserAuthToken>()
            .ok_or_else(required("auth token"))?;
        self.db
            .trx(|c| {
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
        _request: Request<UpsertConfigurationRequest>,
    ) -> Result<Response<UpsertConfigurationResponse>, Status> {
        Err(Status::unimplemented(""))
    }

    async fn get_configuration(
        &self,
        _request: Request<GetConfigurationRequest>,
    ) -> Result<Response<GetConfigurationResponse>, Status> {
        Err(Status::unimplemented(""))
    }
}
