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

use std::borrow::Cow;
use std::sync::Arc;

use axum::http::HeaderValue;
use axum::Extension;
use derive_more::Deref;
use tonic::codec::CompressionEncoding;
use tonic::metadata::AsciiMetadataValue;
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
pub struct Metadata {
    data: axum::http::HeaderMap,
}

impl Metadata {
    pub fn new() -> Self {
        Self {
            data: axum::http::HeaderMap::new(),
        }
    }

    pub fn insert_http(&mut self, k: &'static str, v: impl Into<HeaderValue>) {
        self.data.insert(k, v.into());
    }

    pub fn insert_grpc(&mut self, k: &'static str, v: impl Into<AsciiMetadataValue>) {
        let ascii = v.into();
        // SAFETY: unwrap here is safe because these bytes were just retrieved from an ASCII string.
        let v = HeaderValue::from_bytes(ascii.as_bytes()).unwrap();
        self.data.insert(k, v);
    }

    pub fn get_http(&self, k: &str) -> Option<&HeaderValue> {
        self.data.get(k)
    }
}

impl Default for Metadata {
    fn default() -> Self {
        Self::new()
    }
}

impl From<tonic::metadata::MetadataMap> for Metadata {
    fn from(value: tonic::metadata::MetadataMap) -> Self {
        Self {
            data: value.into_headers(),
        }
    }
}

impl From<Metadata> for tonic::metadata::MetadataMap {
    fn from(value: Metadata) -> Self {
        Self::from_headers(value.data)
    }
}

impl From<axum::http::header::HeaderMap> for Metadata {
    fn from(data: axum::http::header::HeaderMap) -> Self {
        Self { data }
    }
}

/// Sometimes the http status codes are a more granular error reporting mechanism, and sometimes
/// the tonic status codes are. Compare for example http errors 401 and 403, which both correspond
/// to `tonic::Status::permission_denied`. Therefore, we use this enum here which is more granular
/// than either `hyper::StatusCode` and `tonic::Status`.
pub(crate) enum Status {
    NotFound(Cow<'static, str>),
    AlreadyExists(Cow<'static, str>),
    Forbidden(Cow<'static, str>),
    Unauthorized(Cow<'static, str>),
    FailedPrecondition(Cow<'static, str>),
    InvalidArgument(Cow<'static, str>),
    UnparseableRequest(Cow<'static, str>),
    OutOfRange(Cow<'static, str>),
    Internal(Cow<'static, str>),
}

impl Status {
    pub fn not_found(message: impl Into<Cow<'static, str>>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn already_exists(message: impl Into<Cow<'static, str>>) -> Self {
        Self::AlreadyExists(message.into())
    }

    pub fn forbidden(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Forbidden(message.into())
    }

    pub fn unauthorized(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Unauthorized(message.into())
    }

    pub fn failed_precondition(message: impl Into<Cow<'static, str>>) -> Self {
        Self::FailedPrecondition(message.into())
    }

    pub fn invalid_argument(message: impl Into<Cow<'static, str>>) -> Self {
        Self::InvalidArgument(message.into())
    }

    pub fn unparseable_request(message: impl Into<Cow<'static, str>>) -> Self {
        Self::UnparseableRequest(message.into())
    }

    pub fn out_of_range(message: impl Into<Cow<'static, str>>) -> Self {
        Self::OutOfRange(message.into())
    }

    pub fn internal(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Internal(message.into())
    }

    fn error_grpc(self) -> tonic::Status {
        use Status::*;
        match self {
            NotFound(message) => tonic::Status::not_found(message.into_owned()),
            AlreadyExists(message) => tonic::Status::already_exists(message.into_owned()),
            Forbidden(message) => tonic::Status::permission_denied(message.into_owned()),
            Unauthorized(message) => tonic::Status::unauthenticated(message.into_owned()),
            FailedPrecondition(message) => tonic::Status::failed_precondition(message.into_owned()),
            InvalidArgument(message) => tonic::Status::invalid_argument(message.into_owned()),
            UnparseableRequest(message) => tonic::Status::invalid_argument(message.into_owned()),
            OutOfRange(message) => tonic::Status::out_of_range(message.into_owned()),
            Internal(message) => tonic::Status::internal(message.into_owned()),
        }
    }

    fn error_http(self) -> (hyper::StatusCode, serde_json::Value) {
        use Status::*;
        let body =
            |message: Cow<'static, str>| serde_json::json!({"message": message.into_owned()});
        match self {
            NotFound(message) => (hyper::StatusCode::NOT_FOUND, body(message)),
            AlreadyExists(message) => (hyper::StatusCode::CONFLICT, body(message)),
            Forbidden(message) => (hyper::StatusCode::FORBIDDEN, body(message)),
            Unauthorized(message) => (hyper::StatusCode::UNAUTHORIZED, body(message)),
            FailedPrecondition(message) => (hyper::StatusCode::PRECONDITION_FAILED, body(message)),
            InvalidArgument(message) => (hyper::StatusCode::BAD_REQUEST, body(message)),
            UnparseableRequest(message) => (hyper::StatusCode::UNPROCESSABLE_ENTITY, body(message)),
            OutOfRange(message) => (hyper::StatusCode::RANGE_NOT_SATISFIABLE, body(message)),
            Internal(message) => (hyper::StatusCode::INTERNAL_SERVER_ERROR, body(message)),
        }
    }
}

impl From<Status> for tonic::Status {
    fn from(value: Status) -> Self {
        value.error_grpc()
    }
}

impl From<Status> for crate::http::handlers::Error {
    fn from(value: Status) -> Self {
        let (status, message) = value.error_http();
        crate::http::handlers::Error::new(message, status)
    }
}

pub trait ResponseMessage<T> {
    fn construct(message: T, meta: Metadata) -> Self;
}

impl<T> ResponseMessage<T> for tonic::Response<T> {
    fn construct(message: T, meta: Metadata) -> Self {
        tonic::Response::from_parts(meta.into(), message, Default::default())
    }
}

impl<T> ResponseMessage<T> for axum::Json<T> {
    fn construct(message: T, _meta: Metadata) -> axum::Json<T> {
        axum::Json(message)
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
        .add_service(gzip_service!(DiscoveryServiceServer, grpc.clone()))
        .add_service(gzip_service!(HostServiceServer, grpc.clone()))
        .add_service(gzip_service!(InvitationServiceServer, grpc.clone()))
        .add_service(gzip_service!(KernelServiceServer, grpc.clone()))
        .add_service(gzip_service!(MetricsServiceServer, grpc.clone()))
        .add_service(gzip_service!(NodeServiceServer, grpc.clone()))
        .add_service(gzip_service!(OrgServiceServer, grpc.clone()))
        .add_service(gzip_service!(SubscriptionServiceServer, grpc.clone()))
        .add_service(gzip_service!(UserServiceServer, grpc))
}
