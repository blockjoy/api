use api::server;
use diesel::Connection;
use diesel_migrations::MigrationHarness;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    migrate();

    tracing_subscriber::registry()
        .with(fmt::layer().with_ansi(false))
        .with(EnvFilter::from_default_env())
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tracing::info!("Starting server...");
            server::start().await
        })
}

fn migrate() {
    let db_url = api::auth::key_provider::KeyProvider::get_var("DATABASE_URL")
        .expect("DATABASE_URL not set")
        .to_string();
    diesel::PgConnection::establish(&db_url)
        .expect("Could not migrate database!")
        .run_pending_migrations(api::MIGRATIONS)
        .expect("Failed to run migrations");
}
