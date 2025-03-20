pub mod api_key;
pub mod archive;
pub mod auth;
pub mod bundle;
pub mod command;
pub mod crypt;
pub mod discovery;
pub mod host;
pub mod image;
pub mod invitation;
pub mod metrics;
pub mod middleware;
pub mod node;
pub mod org;
pub mod protocol;
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

use axum::Extension;
use axum::http::HeaderValue;
use axum::routing::Router;
use derive_more::Deref;
use tonic::codec::CompressionEncoding;
use tonic::metadata::AsciiMetadataValue;
use tonic::service::Routes;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::cors::{self, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::config::Context;

use self::api::api_key_service_server::ApiKeyServiceServer;
use self::api::archive_service_server::ArchiveServiceServer;
use self::api::auth_service_server::AuthServiceServer;
use self::api::bundle_service_server::BundleServiceServer;
use self::api::command_service_server::CommandServiceServer;
use self::api::crypt_service_server::CryptServiceServer;
use self::api::discovery_service_server::DiscoveryServiceServer;
use self::api::host_service_server::HostServiceServer;
use self::api::image_service_server::ImageServiceServer;
use self::api::invitation_service_server::InvitationServiceServer;
use self::api::metrics_service_server::MetricsServiceServer;
use self::api::node_service_server::NodeServiceServer;
use self::api::org_service_server::OrgServiceServer;
use self::api::protocol_service_server::ProtocolServiceServer;
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

/// Metadata from gRPC or HTTP request headers.
pub struct Metadata {
    headers: axum::http::HeaderMap,
}

impl Metadata {
    pub fn new() -> Self {
        Self {
            headers: axum::http::HeaderMap::new(),
        }
    }

    pub fn insert_http(&mut self, k: &'static str, v: impl Into<HeaderValue>) {
        self.headers.insert(k, v.into());
    }

    pub fn insert_grpc(&mut self, k: &'static str, v: impl Into<AsciiMetadataValue>) {
        let ascii = v.into();
        let v = HeaderValue::from_bytes(ascii.as_bytes()).expect("always ascii");
        self.headers.insert(k, v);
    }

    pub fn get_http(&self, k: &str) -> Option<&HeaderValue> {
        self.headers.get(k)
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
            headers: value.into_headers(),
        }
    }
}

impl From<Metadata> for tonic::metadata::MetadataMap {
    fn from(value: Metadata) -> Self {
        Self::from_headers(value.headers)
    }
}

impl From<axum::http::header::HeaderMap> for Metadata {
    fn from(headers: axum::http::header::HeaderMap) -> Self {
        Self { headers }
    }
}

/// Response status codes returned from both gRPC and http handlers.
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

impl From<Status> for crate::http::handler::Error {
    fn from(value: Status) -> Self {
        let (status, message) = value.error_http();
        crate::http::handler::Error::new(message, status)
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

pub fn router(context: &Arc<Context>) -> Router {
    let grpc = Grpc::new(context.clone());

    let routes = Routes::builder()
        .add_service(gzip_service!(ApiKeyServiceServer, grpc.clone()))
        .add_service(
            ArchiveServiceServer::new(grpc.clone())
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip)
                .max_decoding_message_size(MAX_ARCHIVE_MESSAGE_SIZE),
        )
        .add_service(gzip_service!(AuthServiceServer, grpc.clone()))
        .add_service(gzip_service!(BundleServiceServer, grpc.clone()))
        .add_service(gzip_service!(CommandServiceServer, grpc.clone()))
        .add_service(gzip_service!(CryptServiceServer, grpc.clone()))
        .add_service(gzip_service!(DiscoveryServiceServer, grpc.clone()))
        .add_service(gzip_service!(HostServiceServer, grpc.clone()))
        .add_service(gzip_service!(ImageServiceServer, grpc.clone()))
        .add_service(gzip_service!(InvitationServiceServer, grpc.clone()))
        .add_service(gzip_service!(MetricsServiceServer, grpc.clone()))
        .add_service(gzip_service!(NodeServiceServer, grpc.clone()))
        .add_service(gzip_service!(OrgServiceServer, grpc.clone()))
        .add_service(gzip_service!(ProtocolServiceServer, grpc.clone()))
        .add_service(gzip_service!(UserServiceServer, grpc))
        .clone()
        .routes();

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

    routes
        .into_axum_router()
        .layer(middleware)
        .layer(ConcurrencyLimitLayer::new(
            context.config.grpc.request_concurrency_limit,
        ))
}
