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
pub mod nodes;
pub mod notification;
pub mod orgs;
pub mod users;

#[allow(clippy::large_enum_variant)]
pub mod api {
    tonic::include_proto!("blockjoy.v1");
}

use crate::cloudflare::CloudflareApi;
use crate::models;
use axum::Extension;
use notification::Notifier;
use tonic::transport::server::Router;
use tonic::transport::Server;
use tower::layer::util::{Identity, Stack};
use tower_http::classify::{GrpcErrorsAsFailures, SharedClassifier};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// This struct is used to implement all the gRPC traits as we need them. It can be trivially
/// cloned, both member structs use interal refcounting.
#[derive(Clone)]
struct GrpcImpl {
    db: models::DbPool,
    notifier: Notifier,
    cookbook: super::cookbook::Cookbook,
    dns: CloudflareApi,
}

impl std::ops::Deref for GrpcImpl {
    type Target = models::DbPool;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

type Result<T> = crate::Result<tonic::Response<T>>;
type Resp<T, E = tonic::Status> = std::result::Result<tonic::Response<T>, E>;

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

type TracedServer = Stack<TraceLayer<SharedClassifier<GrpcErrorsAsFailures>>, Identity>;
type DbServer = Stack<Extension<models::DbPool>, TracedServer>;
type CorsServer = Stack<Stack<CorsLayer, DbServer>, Identity>;

pub async fn server(
    db: models::DbPool,
    cloudflare: CloudflareApi,
    cookbook: crate::cookbook::Cookbook,
) -> Router<CorsServer> {
    let notifier = Notifier::new(&db.context.config.mqtt)
        .await
        .expect("Could not set up MQTT notifier!");

    let impler = GrpcImpl {
        db: db.clone(),
        notifier,
        dns: cloudflare,
        cookbook,
    };

    let authentication = api::auth_service_server::AuthServiceServer::new(impler.clone());
    let babel = api::babel_service_server::BabelServiceServer::new(impler.clone());
    let blockchain = api::blockchain_service_server::BlockchainServiceServer::new(impler.clone());
    let command = api::command_service_server::CommandServiceServer::new(impler.clone());
    let cookbook = api::cookbook_service_server::CookbookServiceServer::new(impler.clone());
    let discovery = api::discovery_service_server::DiscoveryServiceServer::new(impler.clone());
    let host = api::host_service_server::HostServiceServer::new(impler.clone());
    let invitation = api::invitation_service_server::InvitationServiceServer::new(impler.clone());
    let key_file = api::key_file_service_server::KeyFileServiceServer::new(impler.clone());
    let metrics = api::metrics_service_server::MetricsServiceServer::new(impler.clone());
    let node = api::node_service_server::NodeServiceServer::new(impler.clone());
    let organization = api::org_service_server::OrgServiceServer::new(impler.clone());
    let user = api::user_service_server::UserServiceServer::new(impler);

    let request_limit = db.context.config.grpc.request_concurrency_limit;

    let cors_rules = CorsLayer::new()
        .allow_headers(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_origin(tower_http::cors::Any);

    let middleware = tower::ServiceBuilder::new()
        .layer(TraceLayer::new_for_grpc())
        .layer(Extension(db))
        .layer(cors_rules)
        .into_inner();

    Server::builder()
        .layer(middleware)
        .concurrency_limit_per_connection(request_limit)
        .add_service(authentication)
        .add_service(babel)
        .add_service(blockchain)
        .add_service(command)
        .add_service(cookbook)
        .add_service(discovery)
        .add_service(host)
        .add_service(invitation)
        .add_service(key_file)
        .add_service(metrics)
        .add_service(node)
        .add_service(organization)
        .add_service(user)
}

/// Function to convert the datetimes from the database into the API representation of a timestamp.
pub fn try_dt_to_ts(
    datetime: chrono::DateTime<chrono::Utc>,
) -> crate::Result<prost_types::Timestamp> {
    const NANOS_PER_SEC: i64 = 1_000_000_000;
    let nanos = datetime.timestamp_nanos();
    let timestamp = prost_types::Timestamp {
        seconds: nanos / NANOS_PER_SEC,
        // This _should_ never fail because 1_000_000_000 fits into an i32, but using `as` was
        // hiding a bug here at first, therefore I have left the `try_into` call here.
        nanos: (nanos % NANOS_PER_SEC).try_into()?,
    };
    Ok(timestamp)
}
