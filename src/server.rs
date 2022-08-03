use crate::auth;
use crate::errors::ApiError;
use crate::models::*;
use crate::new_auth::auth::Authorization;
use crate::new_auth::middleware::authorization::AuthorizationService;
use crate::routes::api_router;
use anyhow::anyhow;
use axum::async_trait;
use axum::extract::{Extension, FromRequest, RequestParts};
use log::{debug, warn};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::borrow::Cow;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::auth::AsyncRequireAuthorizationLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub type DbPool = Arc<PgPool>;

#[async_trait]
impl<B> FromRequest<B> for Authentication
where
    B: Send,
{
    type Rejection = ApiError;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        if let Some(token) = req
            .headers()
            .get("Authorization")
            .and_then(|hv| hv.to_str().ok())
            .and_then(|hv| {
                let words = hv.split("Bearer").collect::<Vec<&str>>();
                let token = words.get(1).map(|w| w.trim());
                token.map(Cow::Borrowed)
            })
        {
            let api_service_secret =
                std::env::var("API_SERVICE_SECRET").unwrap_or_else(|_| "".into());
            let is_service_token = !api_service_secret.is_empty() && token == api_service_secret;

            if token.starts_with("eyJ") {
                debug!("JWT Auth in Bearer.");
                if let Ok(auth::JwtValidationStatus::Valid(auth_data)) =
                    auth::validate_jwt(token.as_ref())
                {
                    if let Ok(role) = UserRole::from_str(&auth_data.user_role) {
                        return Ok(Self::User(UserAuthInfo {
                            id: auth_data.user_id,
                            role,
                        }));
                    }
                }
            } else if is_service_token {
                debug!("Service Auth in Bearer.");
                return Ok(Self::Service(token.as_ref().to_string()));
            } else {
                debug!("Host Auth in Bearer.");
                return Ok(Self::Host(token.as_ref().to_string()));
            };
        };

        warn!(
            "Invalid token auth: {:?} - {:?}",
            req.headers().get("Authorization"),
            req.uri().path()
        );
        Err(ApiError::InvalidAuthentication(anyhow!(
            "invalid authentication credentials"
        )))
    }
}

pub async fn start() -> anyhow::Result<()> {
    let db_url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");

    let db_max_conn: u32 = std::env::var("DB_MAX_CONN")
        .unwrap_or_else(|_| "10".to_string())
        .parse()
        .unwrap();
    let db_min_conn: u32 = std::env::var("DB_MIN_CONN")
        .unwrap_or_else(|_| "2".to_string())
        .parse()
        .unwrap();

    let enforcer = Authorization::new().await.unwrap();
    let auth_service = AuthorizationService::new(enforcer);
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let bind_ip = std::env::var("BIND_IP").unwrap_or_else(|_| "0.0.0.0".to_string());
    let addr = format!("{}:{}", bind_ip, port);

    let db = PgPoolOptions::new()
        .max_connections(db_max_conn)
        .min_connections(db_min_conn)
        .max_lifetime(Some(Duration::from_secs(60 * 60 * 24)))
        .idle_timeout(Some(Duration::from_secs(60 * 2)))
        .connect(&db_url)
        .await
        .expect("Could not create db connection pool.");

    let app = api_router()
        .layer(
            CorsLayer::new()
                .allow_headers(Any)
                .allow_methods(Any)
                .allow_origin(Any),
        )
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(Extension(Arc::new(db)))
        .layer(AsyncRequireAuthorizationLayer::new(auth_service));

    Ok(axum::Server::bind(&addr.parse()?)
        .serve(app.into_make_service())
        .await?)
}
