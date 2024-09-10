pub mod api_key;
pub mod auth;
pub mod blockchain;
pub mod blockchain_archive;
pub mod bundle;
pub mod command;
pub mod crypt;
pub mod discovery;
pub mod host;
pub mod invitation;
pub mod metrics;
pub mod middleware;
pub mod node;
pub mod org;
pub mod subscription;
pub mod user;

const MAX_ARCHIVE_MESSAGE_SIZE: usize = 150 * 1024 * 1024;

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
use tonic::codec::CompressionEncoding;
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
use self::api::crypt_service_server::CryptServiceServer;
use self::api::discovery_service_server::DiscoveryServiceServer;
use self::api::host_service_server::HostServiceServer;
use self::api::invitation_service_server::InvitationServiceServer;
use self::api::metrics_service_server::MetricsServiceServer;
use self::api::node_service_server::NodeServiceServer;
use self::api::org_service_server::OrgServiceServer;
use self::api::subscription_service_server::SubscriptionServiceServer;
use self::api::user_service_server::UserServiceServer;
use self::middleware::MetricsLayer;

#[derive(Clone, Deref)]
struct Grpc {
    #[deref]
    pub context: Arc<Context>,
}

impl Grpc {
    const fn new(context: Arc<Context>) -> Self {
        Grpc { context }
    }
}

macro_rules! gzip_service {
    ($service:ident, $grpc:expr) => {
        $service::new($grpc)
            .accept_compressed(CompressionEncoding::Gzip)
            .send_compressed(CompressionEncoding::Gzip)
    };
}

type TraceServer = Stack<TraceLayer<SharedClassifier<GrpcErrorsAsFailures>>, Identity>;
type MetricsServer = Stack<MetricsLayer, TraceServer>;
type PoolServer = Stack<Extension<Pool>, MetricsServer>;
type CorsServer = Stack<Stack<CorsLayer, PoolServer>, Identity>;

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
        .add_service(gzip_service!(ApiKeyServiceServer, grpc.clone()))
        .add_service(gzip_service!(AuthServiceServer, grpc.clone()))
        .add_service(gzip_service!(BlockchainServiceServer, grpc.clone()))
        .add_service(
            BlockchainArchiveServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip)
                .max_decoding_message_size(MAX_ARCHIVE_MESSAGE_SIZE),
        )
        .add_service(gzip_service!(BundleServiceServer, grpc.clone()))
        .add_service(gzip_service!(CommandServiceServer, grpc.clone()))
        .add_service(gzip_service!(CryptServiceServer, grpc.clone()))
        .add_service(gzip_service!(DiscoveryServiceServer, grpc.clone()))
        .add_service(gzip_service!(HostServiceServer, grpc.clone()))
        .add_service(gzip_service!(InvitationServiceServer, grpc.clone()))
        .add_service(gzip_service!(MetricsServiceServer, grpc.clone()))
        .add_service(gzip_service!(NodeServiceServer, grpc.clone()))
        .add_service(gzip_service!(OrgServiceServer, grpc.clone()))
        .add_service(gzip_service!(SubscriptionServiceServer, grpc.clone()))
        .add_service(gzip_service!(UserServiceServer, grpc))
}
