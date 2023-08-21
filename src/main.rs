use anyhow::{anyhow, Context as _, Result};
use blockvisor_api::config::{Config, Context};
use blockvisor_api::server;
use diesel::{Connection, PgConnection};
use diesel_migrations::MigrationHarness;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let context = Context::new().await?;
    run_migrations(&context.config)?;

    context.config.log.start()?;

    info!("Starting server...");
    server::start(context).await?;

    opentelemetry::global::shutdown_tracer_provider();

    Ok(())
}

fn run_migrations(config: &Config) -> Result<()> {
    PgConnection::establish(config.database.url.as_str())
        .context("failed to establish db connection")?
        .run_pending_migrations(blockvisor_api::database::MIGRATIONS)
        .map(|_versions| ())
        .map_err(|err| anyhow!("failed to run db migrations: {err}"))
}
