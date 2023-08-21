pub mod api_key;
pub mod auth;
pub mod babel;
pub mod blockchains;
pub mod commands;
pub mod cookbook;
pub mod discovery;
pub mod helpers;
pub mod hosts;
pub mod invitations;
pub mod key_files;
pub mod metrics;
pub mod middleware;
pub mod nodes;
pub mod orgs;
pub mod subscription;
pub mod users;

pub mod api {
    tonic::include_proto!("blockjoy.v1");
}

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
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::config::Context;
use crate::database::Pool;

use self::middleware::MetricsLayer;

/// This macro bails out of the current function with a `tonic::Status::permission_denied` error.
/// The arguments that can be supplied here are the same as the arguments to the format macro.
macro_rules! forbidden {
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

use forbidden;

type Result<T> = crate::Result<tonic::Response<T>>;
type Resp<T, E = tonic::Status> = std::result::Result<tonic::Response<T>, E>;
type TraceServer = Stack<TraceLayer<SharedClassifier<GrpcErrorsAsFailures>>, Identity>;
type MetricsServer = Stack<MetricsLayer, TraceServer>;
type PoolServer = Stack<Extension<Pool>, MetricsServer>;
type CorsServer = Stack<Stack<CorsLayer, PoolServer>, Identity>;

/// This struct implements all the gRPC service traits.
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

pub async fn server(context: Arc<Context>) -> Router<CorsServer> {
    let grpc = Grpc::new(context.clone());

    let api_key = api::api_key_service_server::ApiKeyServiceServer::new(grpc.clone());
    let authentication = api::auth_service_server::AuthServiceServer::new(grpc.clone());
    let babel = api::babel_service_server::BabelServiceServer::new(grpc.clone());
    let blockchain = api::blockchain_service_server::BlockchainServiceServer::new(grpc.clone());
    let bundle = api::bundle_service_server::BundleServiceServer::new(grpc.clone());
    let cookbook = api::cookbook_service_server::CookbookServiceServer::new(grpc.clone());
    let command = api::command_service_server::CommandServiceServer::new(grpc.clone());
    let discovery = api::discovery_service_server::DiscoveryServiceServer::new(grpc.clone());
    let host = api::host_service_server::HostServiceServer::new(grpc.clone());
    let invitation = api::invitation_service_server::InvitationServiceServer::new(grpc.clone());
    let kernel = api::kernel_service_server::KernelServiceServer::new(grpc.clone());
    let key_file = api::key_file_service_server::KeyFileServiceServer::new(grpc.clone());
    let manifest = api::manifest_service_server::ManifestServiceServer::new(grpc.clone());
    let metrics = api::metrics_service_server::MetricsServiceServer::new(grpc.clone());
    let node = api::node_service_server::NodeServiceServer::new(grpc.clone());
    let organization = api::org_service_server::OrgServiceServer::new(grpc.clone());
    let subscription =
        api::subscription_service_server::SubscriptionServiceServer::new(grpc.clone());
    let user = api::user_service_server::UserServiceServer::new(grpc);

    let cors_rules = CorsLayer::new()
        .allow_headers(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_origin(tower_http::cors::Any);

    let middleware = tower::ServiceBuilder::new()
        .layer(TraceLayer::new_for_grpc())
        .layer(MetricsLayer)
        .layer(Extension(context.pool.clone()))
        .layer(cors_rules)
        .into_inner();

    Server::builder()
        .layer(middleware)
        .concurrency_limit_per_connection(context.config.grpc.request_concurrency_limit)
        .add_service(api_key)
        .add_service(authentication)
        .add_service(babel)
        .add_service(blockchain)
        .add_service(bundle)
        .add_service(cookbook)
        .add_service(command)
        .add_service(discovery)
        .add_service(host)
        .add_service(invitation)
        .add_service(kernel)
        .add_service(key_file)
        .add_service(manifest)
        .add_service(metrics)
        .add_service(node)
        .add_service(organization)
        .add_service(subscription)
        .add_service(user)
}
