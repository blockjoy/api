pub mod authentication_service;
pub mod command_flow;
pub mod convert;
pub mod helpers;
pub mod host_service;
pub mod key_file_service;
pub mod metrics_service;
pub mod notification;
pub mod organization_service;
pub mod ui_blockchain_service;
pub mod ui_command_service;
pub mod ui_dashboard_service;
pub mod ui_host_provision_service;
pub mod ui_host_service;
pub mod ui_invitation_service;
pub mod ui_node_service;
pub mod ui_update_service;
pub mod user_service;

#[allow(clippy::derive_partial_eq_without_eq)]
pub mod blockjoy {
    tonic::include_proto!("blockjoy.api.v1");
}

#[allow(clippy::derive_partial_eq_without_eq)]
pub mod blockjoy_ui {
    tonic::include_proto!("blockjoy.api.ui_v1");
}

use self::blockjoy::metrics_service_server::MetricsServiceServer;
use crate::auth::middleware::AuthorizationService;
use crate::auth::{
    unauthenticated_paths::UnauthenticatedPaths, Authorization, JwtToken, TokenType,
    UserRefreshToken,
};
use crate::errors::{ApiError, Result as ApiResult};
use crate::grpc::authentication_service::AuthenticationServiceImpl;
use crate::grpc::blockjoy::command_flow_server::CommandFlowServer;
use crate::grpc::blockjoy::key_files_server::KeyFilesServer;
use crate::grpc::blockjoy_ui::authentication_service_server::AuthenticationServiceServer;
use crate::grpc::blockjoy_ui::blockchain_service_server::BlockchainServiceServer;
use crate::grpc::blockjoy_ui::command_service_server::CommandServiceServer;
use crate::grpc::blockjoy_ui::dashboard_service_server::DashboardServiceServer;
use crate::grpc::blockjoy_ui::host_provision_service_server::HostProvisionServiceServer;
use crate::grpc::blockjoy_ui::host_service_server::HostServiceServer;
use crate::grpc::blockjoy_ui::invitation_service_server::InvitationServiceServer;
use crate::grpc::blockjoy_ui::node_service_server::NodeServiceServer;
use crate::grpc::blockjoy_ui::organization_service_server::OrganizationServiceServer;
use crate::grpc::blockjoy_ui::update_service_server::UpdateServiceServer;
use crate::grpc::blockjoy_ui::user_service_server::UserServiceServer;
use crate::grpc::command_flow::CommandFlowServerImpl;
use crate::grpc::key_file_service::KeyFileServiceImpl;
use crate::grpc::metrics_service::MetricsServiceImpl;
use crate::grpc::organization_service::OrganizationServiceImpl;
use crate::grpc::ui_blockchain_service::BlockchainServiceImpl;
use crate::grpc::ui_command_service::CommandServiceImpl;
use crate::grpc::ui_dashboard_service::DashboardServiceImpl;
use crate::grpc::ui_host_provision_service::HostProvisionServiceImpl;
use crate::grpc::ui_host_service::HostServiceImpl;
use crate::grpc::ui_invitation_service::InvitationServiceImpl;
use crate::grpc::ui_node_service::NodeServiceImpl;
use crate::grpc::ui_update_service::UpdateServiceImpl;
use crate::grpc::user_service::UserServiceImpl;
use crate::models;
use anyhow::anyhow;
use axum::Extension;
use blockjoy::hosts_server::HostsServer;
use chrono::NaiveDateTime;
use host_service::HostsServiceImpl;
use std::env;
use tonic::metadata::errors::InvalidMetadataValue;
use tonic::transport::server::Router;
use tonic::transport::Server;
use tower::layer::util::{Identity, Stack};
use tower_http::auth::AsyncRequireAuthorizationLayer;
use tower_http::classify::{GrpcErrorsAsFailures, SharedClassifier};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub async fn server(
    db: models::DbPool,
) -> Router<
    Stack<
        Stack<
            CorsLayer,
            Stack<
                AsyncRequireAuthorizationLayer<AuthorizationService>,
                Stack<
                    Extension<UnauthenticatedPaths>,
                    Stack<
                        Extension<models::DbPool>,
                        Stack<TraceLayer<SharedClassifier<GrpcErrorsAsFailures>>, Identity>,
                    >,
                >,
            >,
        >,
        Identity,
    >,
> {
    // Add unauthenticated paths. TODO: Should this reside in some config file?
    let unauthenticated = UnauthenticatedPaths::new(vec![
        "/blockjoy.api.v1.Hosts/Provision",
        "/blockjoy.api.ui_v1.AuthenticationService/Login",
        "/blockjoy.api.ui_v1.UserService/Create",
        "/blockjoy.api.ui_v1.AuthenticationService/ResetPassword",
    ]);
    let enforcer = Authorization::new()
        .await
        .expect("Could not create Authorization!");
    let auth_service = AuthorizationService::new(enforcer);
    let h_service = HostsServer::new(HostsServiceImpl::new(db.clone()));
    let c_service = CommandFlowServer::new(CommandFlowServerImpl::new(db.clone()));
    let k_service = KeyFilesServer::new(KeyFileServiceImpl::new(db.clone()));
    let m_service = MetricsServiceServer::new(MetricsServiceImpl::new(db.clone()));
    let ui_auth_service =
        AuthenticationServiceServer::new(AuthenticationServiceImpl::new(db.clone()));
    let ui_org_service = OrganizationServiceServer::new(OrganizationServiceImpl::new(db.clone()));
    let ui_user_service = UserServiceServer::new(UserServiceImpl::new(db.clone()));
    let ui_host_service = HostServiceServer::new(HostServiceImpl::new(db.clone()));
    let ui_hostprovision_service =
        HostProvisionServiceServer::new(HostProvisionServiceImpl::new(db.clone()));
    let ui_command_service = CommandServiceServer::new(CommandServiceImpl::new(db.clone()));
    let ui_node_service = NodeServiceServer::new(NodeServiceImpl::new(db.clone()));
    let ui_update_service = UpdateServiceServer::new(UpdateServiceImpl::new(db.clone()));
    let ui_dashboard_service = DashboardServiceServer::new(DashboardServiceImpl::new(db.clone()));
    let ui_blockchain_service =
        BlockchainServiceServer::new(BlockchainServiceImpl::new(db.clone()));
    let ui_invitation_service =
        InvitationServiceServer::new(InvitationServiceImpl::new(db.clone()));

    let middleware = tower::ServiceBuilder::new()
        .layer(TraceLayer::new_for_grpc())
        // TODO: Check if DB extension is still needed
        .layer(Extension(db.clone()))
        .layer(Extension(unauthenticated))
        .layer(AsyncRequireAuthorizationLayer::new(auth_service))
        .layer(
            CorsLayer::new()
                .allow_headers(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_origin(tower_http::cors::Any),
        )
        .into_inner();

    Server::builder()
        .layer(middleware)
        .concurrency_limit_per_connection(rate_limiting_settings())
        .add_service(h_service)
        .add_service(c_service)
        .add_service(k_service)
        .add_service(m_service)
        .add_service(ui_auth_service)
        .add_service(ui_org_service)
        .add_service(ui_user_service)
        .add_service(ui_host_service)
        .add_service(ui_hostprovision_service)
        .add_service(ui_node_service)
        .add_service(ui_command_service)
        .add_service(ui_update_service)
        .add_service(ui_dashboard_service)
        .add_service(ui_blockchain_service)
        .add_service(ui_invitation_service)
}

fn rate_limiting_settings() -> usize {
    env::var("REQUEST_CONCURRENCY_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(32)
}

pub fn response_with_refresh_token<ResponseBody>(
    token: Option<String>,
    inner: ResponseBody,
) -> ApiResult<tonic::Response<ResponseBody>> {
    let mut response = tonic::Response::new(inner);

    if let Some(token) = token {
        // here auth fails, if refresh token is expired
        let refresh_token = UserRefreshToken::from_encoded::<UserRefreshToken>(
            token.as_str(),
            TokenType::UserRefresh,
            true,
        )?;
        let exp = NaiveDateTime::from_timestamp_opt(refresh_token.get_expiration(), 0).ok_or_else(
            || ApiError::UnexpectedError(anyhow!("Invalid timestamp while creating refresh token")),
        )?;
        // let exp = "Fri, 09 Jan 2026 03:15:14 GMT";
        let exp = exp.format("%a, %d %b %Y %H:%M:%S GMT").to_string();

        let raw_cookie = format!(
            "refresh={}; path=/; expires={}; secure; HttpOnly; SameSite=Lax",
            token, exp
        );
        let cookie = raw_cookie.parse().map_err(|e: InvalidMetadataValue| {
            tracing::error!("error creating cookie: {e:?}");
            tonic::Status::internal(e.to_string())
        })?;

        tracing::debug!("Setting refresh cookie");

        response.metadata_mut().insert("set-cookie", cookie);
    } else {
        tracing::debug!("NOT setting refresh cookie as no refresh token is available");
    }

    Ok(response)
}

pub fn get_refresh_token<B>(request: &tonic::Request<B>) -> Option<String> {
    request
        .extensions()
        .get::<UserRefreshToken>()
        .and_then(|t| t.encode().ok())
}
