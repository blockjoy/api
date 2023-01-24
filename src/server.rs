use crate::auth::key_provider::KeyProvider;
use crate::grpc::server as grpc_server;
use crate::http::server as http_server;
use crate::hybrid_server::hybrid as hybrid_server;
use crate::models;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

pub async fn start() -> anyhow::Result<()> {
    let db_url = KeyProvider::get_var("DATABASE_URL")?.to_string();
    let db_max_conn: u32 = std::env::var("DB_MAX_CONN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let db_min_conn: u32 = std::env::var("DB_MIN_CONN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let bind_ip = std::env::var("BIND_IP").unwrap_or_else(|_| "0.0.0.0".to_string());
    let addr = format!("{}:{}", bind_ip, port);

    let db = models::DbPool::new(
        PgPoolOptions::new()
            .max_connections(db_max_conn)
            .min_connections(db_min_conn)
            .max_lifetime(Duration::from_secs(60 * 60 * 24))
            .idle_timeout(Duration::from_secs(60 * 2))
            .connect(&db_url)
            .await
            .expect("Could not create db connection pool."),
    );

    let rest = http_server(db.clone()).await.into_make_service();
    let grpc = grpc_server(db).await.into_service();
    let hybrid = hybrid_server(rest, grpc);

    Ok(axum::Server::bind(&addr.parse()?).serve(hybrid).await?)
}
