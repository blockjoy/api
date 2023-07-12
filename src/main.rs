use blockvisor_api::config::{Config, Context};
use blockvisor_api::server;
use diesel::Connection;
use diesel_migrations::MigrationHarness;
use tracing::info;
use tracing_log::LogTracer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let context = Context::new().await?;
    migrate(&context.config);

    info!("Starting server...");
    server::start(context).await?;

    Ok(())
}

fn init_tracing() {
    LogTracer::init().unwrap();

    let env = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt = fmt::Layer::default().with_ansi(false);
    let registry = Registry::default().with(env).with(fmt);

    tracing::subscriber::set_global_default(registry).unwrap();
}

fn migrate(config: &Config) {
    diesel::PgConnection::establish(config.database.url.as_str())
        .expect("Could not migrate database!")
        .run_pending_migrations(blockvisor_api::database::MIGRATIONS)
        .expect("Failed to run migrations");
}
