pub mod authentication_service;
pub mod command_flow;
pub mod convert;
pub mod helpers;
pub mod host_service;
pub mod notification;
pub mod organization_service;
pub mod ui_command_service;
pub mod ui_dashboard_service;
pub mod ui_host_provision_service;
pub mod ui_host_service;
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

use crate::auth::middleware::AuthorizationService;
use crate::auth::{unauthenticated_paths::UnauthenticatedPaths, Authorization};
use crate::grpc::authentication_service::AuthenticationServiceImpl;
use crate::grpc::blockjoy::command_flow_server::CommandFlowServer;
use crate::grpc::blockjoy_ui::authentication_service_server::AuthenticationServiceServer;
use crate::grpc::blockjoy_ui::command_service_server::CommandServiceServer;
use crate::grpc::blockjoy_ui::dashboard_service_server::DashboardServiceServer;
use crate::grpc::blockjoy_ui::host_provision_service_server::HostProvisionServiceServer;
use crate::grpc::blockjoy_ui::host_service_server::HostServiceServer;
use crate::grpc::blockjoy_ui::node_service_server::NodeServiceServer;
use crate::grpc::blockjoy_ui::organization_service_server::OrganizationServiceServer;
use crate::grpc::blockjoy_ui::update_service_server::UpdateServiceServer;
use crate::grpc::blockjoy_ui::user_service_server::UserServiceServer;
use crate::grpc::command_flow::CommandFlowServerImpl;
use crate::grpc::notification::ChannelNotifier;
use crate::grpc::organization_service::OrganizationServiceImpl;
use crate::grpc::ui_command_service::CommandServiceImpl;
use crate::grpc::ui_dashboard_service::DashboardServiceImpl;
use crate::grpc::ui_host_provision_service::HostProvisionServiceImpl;
use crate::grpc::ui_host_service::HostServiceImpl;
use crate::grpc::ui_node_service::NodeServiceImpl;
use crate::grpc::ui_update_service::UpdateServiceImpl;
use crate::grpc::user_service::UserServiceImpl;
use crate::server::DbPool;
use axum::Extension;
use blockjoy::hosts_server::HostsServer;
use host_service::HostsServiceImpl;
use sqlx::PgPool;
use std::env;
use std::sync::Arc;
use tonic::transport::server::Router;
use tonic::transport::Server;
use tower::layer::util::{Identity, Stack};
use tower_http::auth::AsyncRequireAuthorizationLayer;
use tower_http::classify::{GrpcErrorsAsFailures, SharedClassifier};
use tower_http::trace::TraceLayer;

pub async fn server(
    db: DbPool,
) -> Router<
    Stack<
        Stack<
            AsyncRequireAuthorizationLayer<AuthorizationService>,
            Stack<
                Extension<UnauthenticatedPaths>,
                Stack<
                    Extension<Arc<PgPool>>,
                    Stack<TraceLayer<SharedClassifier<GrpcErrorsAsFailures>>, Identity>,
                >,
            >,
        >,
        Identity,
    >,
> {
    // Create channel notifier to send messages from one task to another
    let notifier = ChannelNotifier::create();

    // Add unauthenticated paths. TODO: Should this reside in some config file?
    let unauthenticated = UnauthenticatedPaths::new(vec![
        "/blockjoy.api.v1.Hosts/Provision",
        "/blockjoy.api.ui_v1.AuthenticationService/Login",
        "/blockjoy.api.ui_v1.UserService/ResetPassword",
        "/blockjoy.api.ui_v1.UserService/Create",
    ]);
    let enforcer = Authorization::new().await.unwrap();
    let auth_service = AuthorizationService::new(enforcer);
    let h_service = HostsServer::new(HostsServiceImpl::new(db.clone()));
    let c_service =
        CommandFlowServer::new(CommandFlowServerImpl::new(db.clone(), notifier.clone()));
    let ui_auth_service =
        AuthenticationServiceServer::new(AuthenticationServiceImpl::new(db.clone()));
    let ui_org_service = OrganizationServiceServer::new(OrganizationServiceImpl::new(db.clone()));
    let ui_user_service = UserServiceServer::new(UserServiceImpl::new(db.clone()));
    let ui_host_service = HostServiceServer::new(HostServiceImpl::new(db.clone()));
    let ui_hostprovision_service =
        HostProvisionServiceServer::new(HostProvisionServiceImpl::new(db.clone()));
    let ui_command_service =
        CommandServiceServer::new(CommandServiceImpl::new(db.clone(), notifier.clone()));
    let ui_node_service = NodeServiceServer::new(NodeServiceImpl::new(db.clone()));
    let ui_update_service = UpdateServiceServer::new(UpdateServiceImpl::new(db.clone(), notifier));
    let ui_dashboard_service = DashboardServiceServer::new(DashboardServiceImpl::new(db.clone()));
    let middleware = tower::ServiceBuilder::new()
        .layer(TraceLayer::new_for_grpc())
        .layer(Extension(db.clone()))
        .layer(Extension(unauthenticated))
        .layer(AsyncRequireAuthorizationLayer::new(auth_service))
        .into_inner();

    Server::builder()
        .layer(middleware)
        .concurrency_limit_per_connection(rate_limiting_settings())
        .add_service(h_service)
        .add_service(c_service)
        .add_service(ui_auth_service)
        .add_service(ui_org_service)
        .add_service(ui_user_service)
        .add_service(ui_host_service)
        .add_service(ui_hostprovision_service)
        .add_service(ui_node_service)
        .add_service(ui_command_service)
        .add_service(ui_update_service)
        .add_service(ui_dashboard_service)
}

fn rate_limiting_settings() -> usize {
    env::var("REQUEST_CONCURRENCY_LIMIT")
        .unwrap()
        .parse::<usize>()
        .unwrap_or(32)
}
