pub mod authentication_service;
pub mod command_service;
pub mod convert;
pub mod helpers;
pub mod host_service;
pub mod key_file_service;
pub mod metrics_service;
pub mod node_service;
pub mod notification;
pub mod organization_service;
pub mod service_discovery;
pub mod ui_blockchain_service;
pub mod ui_command_service;
pub mod ui_dashboard_service;
pub mod ui_host_provision_service;
pub mod ui_host_service;
pub mod ui_invitation_service;
pub mod ui_node_service;
pub mod user_service;

#[allow(clippy::large_enum_variant)]
pub mod blockjoy {
    tonic::include_proto!("blockjoy.api.v1");
}

pub mod blockjoy_ui {
    tonic::include_proto!("blockjoy.api.ui_v1");
}

use self::blockjoy::metrics_service_server::MetricsServiceServer;
use self::notification::Notifier;
use crate::auth::middleware::AuthorizationService;
use crate::auth::{
    unauthenticated_paths::UnauthenticatedPaths, Authorization, JwtToken, TokenType,
    UserRefreshToken,
};
use crate::errors::{ApiError, Result as ApiResult};
// use crate::grpc::authentication_service::AuthenticationServiceImpl;
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
use crate::grpc::blockjoy_ui::user_service_server::UserServiceServer;
use crate::{grpc, models};
use anyhow::anyhow;
use axum::Extension;
use chrono::NaiveDateTime;
use std::env;
use tonic::metadata::errors::InvalidMetadataValue;
use tonic::transport::server::Router;
use tonic::transport::Server;
use tower::layer::util::{Identity, Stack};
use tower_http::auth::AsyncRequireAuthorizationLayer;
use tower_http::classify::{GrpcErrorsAsFailures, SharedClassifier};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// This struct is used to implement all the gRPC traits as we need them. It can be trivially
/// cloned, both member structs use interal refcounting.
#[derive(Clone)]
struct GrpcImpl {
    db: models::DbPool,
    notifier: Notifier,
}

#[macro_export]
macro_rules! bail_unauthorized {
    ($msg:literal $(,)?) => {
        return Err(tonic::Status::permission_denied(format!($msg)).into())
    };
    ($err:expr $(,)?) => {
        return Err(tonic::Status::permission_denied(format!($err)).into())
    };
    ($fmt:expr, $($arg:tt)*) => {
        return Err(tonic::Status::permission_denied(format!($fmt, $($arg)*)).into())
    };
}

use bail_unauthorized;

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
        // This path is unauthenticated because you need to have the OTP to create a new host, and
        // that is used instead of the normal machinery.
        "/blockjoy.api.v1.HostService/Provision",
        // The following paths are for users to create and manage their accounts, so should not
        // require authentication either.
        "/blockjoy.api.ui_v1.AuthenticationService/Login",
        "/blockjoy.api.ui_v1.UserService/Create",
        "/blockjoy.api.ui_v1.AuthenticationService/ResetPassword",
    ]);
    let enforcer = Authorization::new()
        .await
        .expect("Could not create Authorization!");
    let auth_service = AuthorizationService::new(enforcer);
    let notifier = Notifier::new()
        .await
        .expect("Could not set up MQTT notifier!");
    let impler = GrpcImpl {
        db: db.clone(),
        notifier,
    };

    let discovery_service = grpc::blockjoy::discovery_server::DiscoveryServer::new(impler.clone());
    let command_service = grpc::blockjoy::commands_server::CommandsServer::new(impler.clone());
    let node_service = grpc::blockjoy::node_service_server::NodeServiceServer::new(impler.clone());
    let h_service = grpc::blockjoy::host_service_server::HostServiceServer::new(impler.clone());
    let k_service = KeyFilesServer::new(impler.clone());
    let m_service = MetricsServiceServer::new(impler.clone());
    let ui_auth_service = AuthenticationServiceServer::new(impler.clone());
    let ui_org_service = OrganizationServiceServer::new(impler.clone());
    let ui_user_service = UserServiceServer::new(impler.clone());
    let ui_host_service = HostServiceServer::new(impler.clone());
    let ui_hostprovision_service = HostProvisionServiceServer::new(impler.clone());
    let ui_command_service = CommandServiceServer::new(impler.clone());
    let ui_node_service = NodeServiceServer::new(impler.clone());
    let ui_dashboard_service = DashboardServiceServer::new(impler.clone());
    let ui_blockchain_service = BlockchainServiceServer::new(impler.clone());
    let ui_invitation_service = InvitationServiceServer::new(impler);

    let middleware = tower::ServiceBuilder::new()
        .layer(TraceLayer::new_for_grpc())
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
        .add_service(discovery_service)
        .add_service(command_service)
        .add_service(node_service)
        .add_service(k_service)
        .add_service(m_service)
        .add_service(ui_auth_service)
        .add_service(ui_org_service)
        .add_service(ui_user_service)
        .add_service(ui_host_service)
        .add_service(ui_hostprovision_service)
        .add_service(ui_node_service)
        .add_service(ui_command_service)
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
) -> ApiResult<tonic::Response<ResponseBody>, tonic::Status> {
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

        let raw_cookie =
            format!("refresh={token}; path=/; expires={exp}; secure; HttpOnly; SameSite=Lax");
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
