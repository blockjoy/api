use std::sync::Arc;
use api::grpc;
use api::http;
use api::multiplex::MultiplexService;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use api::models::DbPool;

async fn db_connection() -> DbPool {
    let db_url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");

    let db_max_conn: u32 = std::env::var("DB_MAX_CONN")
        .unwrap_or_else(|_| "10".to_string())
        .parse()
        .unwrap();
    let db_min_conn: u32 = std::env::var("DB_MIN_CONN")
        .unwrap_or_else(|_| "2".to_string())
        .parse()
        .unwrap();

    let db = PgPoolOptions::new()
        .max_connections(db_max_conn)
        .min_connections(db_min_conn)
        .max_lifetime(Some(Duration::from_secs(60 * 60 * 24)))
        .idle_timeout(Some(Duration::from_secs(60 * 2)))
        .connect(&db_url)
        .await
        .expect("Could not create db connection pool.");

    Arc::new(db)
}

fn build_address() -> String {
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let bind_ip = std::env::var("BIND_IP").unwrap_or_else(|_| "0.0.0.0".to_string());

    format!("{}:{}", bind_ip, port)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let db  = db_connection().await;
    let addr = build_address().parse()?;
    let rest_service = http::server(db.clone());
    let grpc_service = grpc::server(db.clone());
    let service = MultiplexService::new(rest_service, grpc_service);

    tracing::info!("Starting server...");

    Ok(axum::Server::bind(&addr)
        .serve(tower::make::Shared::new(service))
        .await
        .unwrap())
}
