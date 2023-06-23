use blockvisor_api::config::{Config, Context};
use blockvisor_api::server;
use diesel::Connection;
use diesel_migrations::MigrationHarness;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() -> anyhow::Result<()> {
    let context = Context::new()?;

    migrate(&context.config);

    tracing_subscriber::registry()
        .with(fmt::layer().with_ansi(false))
        .with(EnvFilter::from_default_env())
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tracing::info!("Starting server...");
            server::start(context).await
        })
}

fn migrate(config: &Config) {
    diesel::PgConnection::establish(config.database.url.as_str())
        .expect("Could not migrate database!")
        .run_pending_migrations(blockvisor_api::MIGRATIONS)
        .expect("Failed to run migrations");
}
