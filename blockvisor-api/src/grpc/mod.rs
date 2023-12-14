pub mod api_key;
pub mod auth;
pub mod blockchain;
pub mod blockchain_archive;
pub mod bundle;
pub mod command;
pub mod discovery;
pub mod host;
pub mod invitation;
pub mod kernel;
pub mod metrics;
pub mod middleware;
pub mod node;
pub mod org;
pub mod subscription;
pub mod user;

#[allow(clippy::nursery, clippy::pedantic)]
pub mod api {
    tonic::include_proto!("blockjoy.v1");
}

#[allow(clippy::nursery, clippy::pedantic)]
pub mod common {
    tonic::include_proto!("blockjoy.common.v1");

    pub mod v1 {
        pub use super::*;
    }
}

use std::sync::Arc;

use axum::Extension;
use derive_more::Deref;
use tonic::transport::server::Router;
use tonic::transport::Server;
use tower::layer::util::{Identity, Stack};
use tower_http::classify::{GrpcErrorsAsFailures, SharedClassifier};
use tower_http::cors::{self, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::config::Context;
use crate::database::Pool;

use self::api::api_key_service_server::ApiKeyServiceServer;
use self::api::auth_service_server::AuthServiceServer;
use self::api::blockchain_archive_service_server::BlockchainArchiveServiceServer;
use self::api::blockchain_service_server::BlockchainServiceServer;
use self::api::bundle_service_server::BundleServiceServer;
use self::api::command_service_server::CommandServiceServer;
use self::api::discovery_service_server::DiscoveryServiceServer;
use self::api::host_service_server::HostServiceServer;
use self::api::invitation_service_server::InvitationServiceServer;
use self::api::kernel_service_server::KernelServiceServer;
use self::api::metrics_service_server::MetricsServiceServer;
use self::api::node_service_server::NodeServiceServer;
use self::api::org_service_server::OrgServiceServer;
use self::api::subscription_service_server::SubscriptionServiceServer;
use self::api::user_service_server::UserServiceServer;
use self::middleware::MetricsLayer;

type TraceServer = Stack<TraceLayer<SharedClassifier<GrpcErrorsAsFailures>>, Identity>;
type MetricsServer = Stack<MetricsLayer, TraceServer>;
type PoolServer = Stack<Extension<Pool>, MetricsServer>;
type CorsServer = Stack<Stack<CorsLayer, PoolServer>, Identity>;

/// This struct implements all the grpc service traits.
#[derive(Clone, Deref)]
struct Grpc {
    #[deref]
    pub context: Arc<Context>,
}

impl Grpc {
    fn new(context: Arc<Context>) -> Self {
        Grpc { context }
    }
}

pub fn server(context: &Arc<Context>) -> Router<CorsServer> {
    let grpc = Grpc::new(context.clone());

    let cors_rules = CorsLayer::new()
        .allow_headers(cors::Any)
        .allow_methods(cors::Any)
        .allow_origin(cors::Any);

    let middleware = tower::ServiceBuilder::new()
        .layer(TraceLayer::new_for_grpc())
        .layer(MetricsLayer)
        .layer(Extension(context.pool.clone()))
        .layer(cors_rules)
        .into_inner();

    Server::builder()
        .layer(middleware)
        .concurrency_limit_per_connection(context.config.grpc.request_concurrency_limit)
        .add_service(ApiKeyServiceServer::new(grpc.clone()))
        .add_service(AuthServiceServer::new(grpc.clone()))
        .add_service(BlockchainServiceServer::new(grpc.clone()))
        .add_service(BlockchainArchiveServiceServer::new(grpc.clone()))
        .add_service(BundleServiceServer::new(grpc.clone()))
        .add_service(CommandServiceServer::new(grpc.clone()))
        .add_service(DiscoveryServiceServer::new(grpc.clone()))
        .add_service(HostServiceServer::new(grpc.clone()))
        .add_service(InvitationServiceServer::new(grpc.clone()))
        .add_service(KernelServiceServer::new(grpc.clone()))
        .add_service(MetricsServiceServer::new(grpc.clone()))
        .add_service(NodeServiceServer::new(grpc.clone()))
        .add_service(OrgServiceServer::new(grpc.clone()))
        .add_service(SubscriptionServiceServer::new(grpc.clone()))
        .add_service(UserServiceServer::new(grpc))
}
