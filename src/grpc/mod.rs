pub mod authentication;
pub mod blockchains;
pub mod commands;
pub mod discovery;
pub mod helpers;
pub mod host_provisions;
pub mod hosts;
pub mod invitations;
pub mod key_files;
pub mod metrics;
pub mod nodes;
pub mod notification;
pub mod organizations;
pub mod users;

#[allow(clippy::large_enum_variant)]
pub mod api {
    tonic::include_proto!("v1");
}

use crate::auth::{
    middleware::AuthorizationService, unauthenticated_paths::UnauthenticatedPaths, Authorization,
    JwtToken, TokenType, UserRefreshToken,
};
use crate::models;
use axum::Extension;
use chrono::NaiveDateTime;
use notification::Notifier;
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

impl std::ops::Deref for GrpcImpl {
    type Target = models::DbPool;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

type Result<T, E = tonic::Status> = std::result::Result<tonic::Response<T>, E>;

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

type TracedServer = Stack<TraceLayer<SharedClassifier<GrpcErrorsAsFailures>>, Identity>;
type DbServer = Stack<Extension<models::DbPool>, TracedServer>;
type UnauthServer = Stack<Extension<UnauthenticatedPaths>, DbServer>;
type AuthServer = Stack<AsyncRequireAuthorizationLayer<AuthorizationService>, UnauthServer>;
type CorsServer = Stack<Stack<CorsLayer, AuthServer>, Identity>;

pub async fn server(db: models::DbPool) -> Router<CorsServer> {
    // Add unauthenticated paths. TODO: Should this reside in some config file?
    let unauthenticated = UnauthenticatedPaths::new(vec![
        // This path is unauthenticated because you need to have the OTP to create a new host, and
        // that is used instead of the normal machinery.
        "/v1.Hosts/Provision",
        // The following paths are for users to create and manage their accounts, so should not
        // require authentication either.
        "/v1.Authentication/Login",
        "/v1.Authentication/ResetPassword",
        "/v1.Users/Create",
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

    let authentication = api::authentication_server::AuthenticationServer::new(impler.clone());
    // let billing = api::billings_server::BillingsServer::new(impler.clone());
    let blockchain = api::blockchains_server::BlockchainsServer::new(impler.clone());
    let command = api::commands_server::CommandsServer::new(impler.clone());
    let discovery = api::discovery_server::DiscoveryServer::new(impler.clone());
    let host_provision = api::host_provisions_server::HostProvisionsServer::new(impler.clone());
    let host = api::hosts_server::HostsServer::new(impler.clone());
    let invitation = api::invitations_server::InvitationsServer::new(impler.clone());
    let key_file = api::key_files_server::KeyFilesServer::new(impler.clone());
    let metrics = api::metrics_server::MetricsServer::new(impler.clone());
    let node = api::nodes_server::NodesServer::new(impler.clone());
    let organization = api::orgs_server::OrgsServer::new(impler.clone());
    let user = api::users_server::UsersServer::new(impler);

    let cors_rules = CorsLayer::new()
        .allow_headers(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_origin(tower_http::cors::Any);

    let middleware = tower::ServiceBuilder::new()
        .layer(TraceLayer::new_for_grpc())
        .layer(Extension(db))
        .layer(Extension(unauthenticated))
        .layer(AsyncRequireAuthorizationLayer::new(auth_service))
        .layer(cors_rules)
        .into_inner();

    Server::builder()
        .layer(middleware)
        .concurrency_limit_per_connection(rate_limiting_settings())
        .add_service(authentication)
        .add_service(blockchain)
        .add_service(command)
        .add_service(discovery)
        .add_service(host_provision)
        .add_service(host)
        .add_service(invitation)
        .add_service(key_file)
        .add_service(metrics)
        .add_service(node)
        .add_service(organization)
        .add_service(user)
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
) -> Result<ResponseBody> {
    let mut response = tonic::Response::new(inner);

    if let Some(token) = token {
        // here auth fails, if refresh token is expired
        let refresh_token = UserRefreshToken::from_encoded::<UserRefreshToken>(
            token.as_str(),
            TokenType::UserRefresh,
            true,
        )?;
        let exp = NaiveDateTime::from_timestamp_opt(refresh_token.get_expiration(), 0).ok_or_else(
            || crate::Error::unexpected("Invalid timestamp while creating refresh token"),
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

pub fn json_value_to_vec(json: &serde_json::Value) -> crate::Result<Vec<api::FilteredIpAddr>> {
    let arr = json
        .as_array()
        .ok_or_else(|| crate::Error::unexpected("Error deserializing JSON object"))?;
    let mut result = vec![];

    for value in arr {
        let tmp = value
            .as_object()
            .ok_or_else(|| crate::Error::unexpected("Error deserializing JSON array"))?;
        let ip = tmp
            .get("ip")
            .map(|e| e.to_string())
            .ok_or_else(|| crate::Error::unexpected("Can't read IP"))?
            .to_string();
        let description = tmp.get("description").map(|e| e.to_string());

        result.push(api::FilteredIpAddr { ip, description });
    }

    Ok(result)
}
