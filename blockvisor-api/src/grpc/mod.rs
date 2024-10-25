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

use axum::http::HeaderValue;
use axum::Extension;
use derive_more::Deref;
use tonic::codec::CompressionEncoding;
use tonic::metadata::{AsciiMetadataValue, MetadataMap};
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
    const fn new(context: Arc<Context>) -> Self {
        Grpc { context }
    }
}

/// A map of metadata that can either be used for either http or grpc requests.
pub struct NaiveMeta {
    data: axum::http::HeaderMap,
}

impl NaiveMeta {
    pub fn new() -> Self {
        Self {
            data: axum::http::HeaderMap::new(),
        }
    }

    pub fn insert_http(&mut self, k: &'static str, v: impl Into<HeaderValue>) {
        self.data.insert(k, v.into());
    }

    pub fn insert_grpc(&mut self, k: &'static str, v: impl Into<AsciiMetadataValue>) {
        let mut map = MetadataMap::new();
        map.insert(k, v.into());
        self.data.extend(map.into_headers());
    }

    pub fn get_http(&self, k: &str) -> Option<&HeaderValue> {
        self.data.get(k)
    }
}

impl Default for NaiveMeta {
    fn default() -> Self {
        Self::new()
    }
}

impl From<tonic::metadata::MetadataMap> for NaiveMeta {
    fn from(value: tonic::metadata::MetadataMap) -> Self {
        Self {
            data: value.into_headers(),
        }
    }
}

impl From<NaiveMeta> for tonic::metadata::MetadataMap {
    fn from(value: NaiveMeta) -> Self {
        Self::from_headers(value.data)
    }
}

impl From<axum::http::header::HeaderMap> for NaiveMeta {
    fn from(data: axum::http::header::HeaderMap) -> Self {
        Self { data }
    }
}

pub trait ResponseMessage<T> {
    fn construct(message: T, meta: NaiveMeta) -> Self;
}

impl<T> ResponseMessage<T> for tonic::Response<T> {
    fn construct(message: T, meta: NaiveMeta) -> Self {
        tonic::Response::from_parts(meta.into(), message, Default::default())
    }
}

impl<T> ResponseMessage<T> for axum::Json<T> {
    fn construct(message: T, _meta: NaiveMeta) -> axum::Json<T> {
        axum::Json(message)
    }
}

impl ResponseMessage<&'static str> for &'static str {
    fn construct(message: &'static str, _: NaiveMeta) -> &'static str {
        message
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
        .add_service(
            ApiKeyServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            AuthServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            BlockchainServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            BlockchainArchiveServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip)
                .max_decoding_message_size(MAX_ARCHIVE_MESSAGE_SIZE),
        )
        .add_service(
            BundleServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            CommandServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            DiscoveryServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            HostServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            InvitationServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            KernelServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            MetricsServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            NodeServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            OrgServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            SubscriptionServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
        .add_service(
            UserServiceServer::new(grpc)
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
}
