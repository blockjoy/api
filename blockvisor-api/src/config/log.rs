use std::time::Duration;

use displaydoc::Display;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::{
    BaggagePropagator, TextMapCompositePropagator, TraceContextPropagator,
};
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry_sdk::trace::{BatchConfig, Sampler};
use opentelemetry_sdk::{trace, Resource};
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use serde::Deserialize;
use strum::{EnumString, IntoStaticStr};
use thiserror::Error;
use tracing::Subscriber;
use tracing_error::ErrorLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};
use url::Url;

use super::provider::{self, Provider};
use super::HumanTime;

const SERVICE_NAME_DEV: &str = "blockvisor-api-dev";
const SERVICE_NAME_STAGING: &str = "blockvisor-api-staging";
const SERVICE_NAME_PRODUCTION: &str = "blockvisor-api-production";

const LOG_ENVIRONMENT_VAR: &str = "LOG_ENVIRONMENT";
const LOG_ENVIRONMENT_ENTRY: &str = "log.environment";
const LOG_FILTER_VAR: &str = "LOG_FILTER";
const LOG_FILTER_ENTRY: &str = "log.filter";
const LOG_FILTER_DEFAULT: &str = "info";

const OPENTELEMETRY_ENDPOINT_VAR: &str = "OPENTELEMETRY_ENDPOINT";
const OPENTELEMETRY_ENDPOINT_ENTRY: &str = "log.opentelemetry.endpoint";
const OPENTELEMETRY_EXPORT_INTERVAL_VAR: &str = "OPENTELEMETRY_EXPORT_INTERVAL";
const OPENTELEMETRY_EXPORT_INTERVAL_ENTRY: &str = "log.opentelemetry.export_interval";
const OPENTELEMETRY_EXPORT_INTERVAL_DEFAULT: Duration = Duration::from_secs(5);

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse OpentelemetryConfig: {0}
    Opentelemetry(#[from] OpentelemetryError),
    /// Failed to parse {LOG_ENVIRONMENT_ENTRY:?}: {0}
    ParseEnvironment(provider::Error),
    /// Failed to parse {LOG_FILTER_ENTRY:?}: {0}
    ParseFilter(provider::Error),
    /// Failed to start global subscriber: {0}
    StartGlobal(tracing_subscriber::util::TryInitError),
    /// Failed to start opentelemetry tracing: {0}
    StartTracer(opentelemetry::trace::TraceError),
}

#[derive(Clone, Copy, Debug, Deserialize, EnumString, IntoStaticStr)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Environment {
    Dev,
    Staging,
    Production,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub environment: Environment,
    pub filter: String,
    pub opentelemetry: OpentelemetryConfig,
}

impl Config {
    pub fn start(&self) -> Result<(), Error> {
        self.setup_registry(false)?.init();
        Ok(())
    }

    fn setup_registry(&self, ansi: bool) -> Result<impl Subscriber, Error> {
        let env = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(self.filter.clone()));

        global::set_text_map_propagator(TextMapCompositePropagator::new(vec![
            Box::new(TraceContextPropagator::new()),
            Box::new(BaggagePropagator::new()),
        ]));

        let resource = Resource::new(vec![KeyValue::new(SERVICE_NAME, self.service_name())]);
        let trace_config = trace::config()
            .with_resource(resource)
            .with_sampler(Sampler::AlwaysOn);
        let batch_config =
            BatchConfig::default().with_scheduled_delay(*self.opentelemetry.export_interval);

        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(self.opentelemetry.endpoint.clone());
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(exporter)
            .with_trace_config(trace_config)
            .with_batch_config(batch_config)
            .install_batch(Tokio)
            .map_err(Error::StartTracer)?;

        let registry = Registry::default()
            .with(env)
            .with(fmt::Layer::default().with_ansi(ansi))
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .with(ErrorLayer::default());

        Ok(registry)
    }

    pub const fn service_name(&self) -> &'static str {
        match self.environment {
            Environment::Dev => SERVICE_NAME_DEV,
            Environment::Staging => SERVICE_NAME_STAGING,
            Environment::Production => SERVICE_NAME_PRODUCTION,
        }
    }
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let environment = provider
            .read(LOG_ENVIRONMENT_VAR, LOG_ENVIRONMENT_ENTRY)
            .map_err(Error::ParseEnvironment)?;
        let filter = provider
            .read_or(LOG_FILTER_DEFAULT, LOG_FILTER_VAR, LOG_FILTER_ENTRY)
            .map_err(Error::ParseFilter)?;

        Ok(Config {
            environment,
            filter,
            opentelemetry: provider.try_into()?,
        })
    }
}

#[derive(Debug, Display, Error)]
pub enum OpentelemetryError {
    /// Failed to parse {OPENTELEMETRY_ENDPOINT_ENTRY:?}: {0}
    ParseEndpoint(provider::Error),
    /// Failed to parse {OPENTELEMETRY_EXPORT_INTERVAL_ENTRY:?}: {0}
    ParseExportInterval(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryConfig {
    pub endpoint: Url,
    pub export_interval: HumanTime,
}

impl TryFrom<&Provider> for OpentelemetryConfig {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let endpoint = provider
            .read(OPENTELEMETRY_ENDPOINT_VAR, OPENTELEMETRY_ENDPOINT_ENTRY)
            .map_err(OpentelemetryError::ParseEndpoint)?;
        let export_interval = provider
            .read_or(
                OPENTELEMETRY_EXPORT_INTERVAL_DEFAULT,
                OPENTELEMETRY_EXPORT_INTERVAL_VAR,
                OPENTELEMETRY_EXPORT_INTERVAL_ENTRY,
            )
            .map_err(OpentelemetryError::ParseExportInterval)?;

        Ok(OpentelemetryConfig {
            endpoint,
            export_interval,
        })
    }
}
