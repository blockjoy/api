use crate::auth::TokenType;
use crate::grpc::blockjoy_ui::user_service_server::UserService;
use crate::grpc::blockjoy_ui::{
    response_meta, CreateUserRequest, CreateUserResponse, GetConfigurationRequest,
    GetConfigurationResponse, GetUserRequest, GetUserResponse, ResponseMeta, UpdateUserRequest,
    UpdateUserResponse, UpsertConfigurationRequest, UpsertConfigurationResponse, User as GrpcUser,
};
use crate::grpc::helpers::success_response_meta;
use crate::models::{Token, TokenRole, User, UserRequest};
use crate::server::DbPool;
use tonic::{Request, Response, Status};

pub struct UserServiceImpl {
    db: DbPool,
}

impl UserServiceImpl {
    pub fn new(db: DbPool) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl UserService for UserServiceImpl {
    async fn get(
        &self,
        request: Request<GetUserRequest>,
    ) -> Result<Response<GetUserResponse>, Status> {
        let token = request.extensions().get::<Token>().unwrap().token.clone();
        let inner = request.into_inner();
        let user = Token::get_user_for_token(token, TokenType::Login, &self.db).await?;
        let meta = success_response_meta(inner.meta.unwrap().id);
        let response = GetUserResponse {
            meta: Some(meta),
            user: Some(GrpcUser::from(user)),
        };

        Ok(Response::new(response))
    }

    async fn create(
        &self,
        request: Request<CreateUserRequest>,
    ) -> Result<Response<CreateUserResponse>, Status> {
        let inner = request.into_inner();
        let required =
            |name| move || tonic::Status::invalid_argument(format!("`{name}` is required"));
        let user = inner.user.ok_or_else(required("user"))?;
        let user_request = UserRequest {
            email: user.email.ok_or_else(required("email"))?,
            first_name: user.first_name.ok_or_else(required("first_name"))?,
            last_name: user.last_name.ok_or_else(required("last_name"))?,
            password: inner.password,
            password_confirm: inner.password_confirmation,
        };

        match User::create(user_request, &self.db, Some(TokenRole::User)).await {
            Ok(new_user) => {
                let meta = ResponseMeta {
                    status: i32::from(response_meta::Status::Success),
                    origin_request_id: inner.meta.ok_or_else(required("meta"))?.id,
                    messages: vec![new_user.id.to_string()],
                    pagination: None,
                };
                let response = CreateUserResponse { meta: Some(meta) };

                Ok(Response::new(response))
            }
            Err(e) => Err(Status::from(e)),
        }
    }

    async fn update(
        &self,
        _request: Request<UpdateUserRequest>,
    ) -> Result<Response<UpdateUserResponse>, Status> {
        Err(Status::unimplemented(""))
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
