use std::sync::{Arc, OnceLock};
use std::time::Duration;

use displaydoc::Display;
use opentelemetry::global;
use opentelemetry::trace::TraceError;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::{LogError, SdkLoggerProvider};
use opentelemetry_sdk::metrics::{MetricError, PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde::Deserialize;
use strum::{EnumString, IntoStaticStr};
use thiserror::Error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};
use url::Url;

use super::HumanTime;
use super::provider::{self, Provider};

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

static INIT_LOG: OnceLock<Arc<Log>> = OnceLock::new();

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse log directive: {0}
    Directive(#[from] tracing_subscriber::filter::ParseError),
    /// Log error: {0}
    LogError(#[from] LogError),
    /// Metric error: {0}
    MetricError(#[from] MetricError),
    /// Failed to parse OpentelemetryConfig: {0}
    OtelConfig(#[from] OpentelemetryError),
    /// Opentelemetry SDK error: {0}
    OtelSdk(#[from] opentelemetry_sdk::error::OTelSdkError),
    /// Failed to parse {LOG_ENVIRONMENT_ENTRY:?}: {0}
    ParseEnvironment(provider::Error),
    /// Failed to parse {LOG_FILTER_ENTRY:?}: {0}
    ParseFilter(provider::Error),
    /// Trace error: {0}
    TraceError(#[from] TraceError),
}

#[derive(Clone, Copy, Debug, Deserialize, EnumString, IntoStaticStr)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Environment {
    Dev,
    Staging,
    Production,
}

pub struct Log {
    pub logger: SdkLoggerProvider,
    pub meter: SdkMeterProvider,
    pub tracer: SdkTracerProvider,
    pub filter: String,
    pub interval: Duration,
}

impl Log {
    pub fn new(config: &Config) -> Arc<Self> {
        #[allow(clippy::significant_drop_tightening)] // clippy bug?
        let log = INIT_LOG.get_or_init(|| {
            let resource = Resource::builder()
                .with_service_name(config.service_name())
                .build();
            let interval = *config.opentelemetry.export_interval;

            let exporter = LogExporter::builder()
                .with_tonic()
                .with_endpoint(config.opentelemetry.endpoint.clone())
                .build()
                .expect("log exporter");
            let logger = SdkLoggerProvider::builder()
                .with_resource(resource.clone())
                .with_batch_exporter(exporter)
                .build();

            let exporter = MetricExporter::builder()
                .with_tonic()
                .with_endpoint(config.opentelemetry.endpoint.clone())
                .build()
                .expect("metric exporter");
            let reader = PeriodicReader::builder(exporter)
                .with_interval(interval)
                .build();
            let meter = SdkMeterProvider::builder()
                .with_resource(resource.clone())
                .with_reader(reader)
                .build();

            let exporter = SpanExporter::builder()
                .with_tonic()
                .with_endpoint(config.opentelemetry.endpoint.clone())
                .build()
                .expect("span exporter");
            let tracer = SdkTracerProvider::builder()
                .with_resource(resource)
                .with_batch_exporter(exporter)
                .build();

            Arc::new(Log {
                logger,
                meter,
                tracer,
                filter: config.filter.clone(),
                interval,
            })
        });

        log.clone()
    }

    pub fn init(&self) -> Result<(), Error> {
        global::set_tracer_provider(self.tracer.clone());
        global::set_meter_provider(self.meter.clone());

        // https://github.com/open-telemetry/opentelemetry-rust/issues/761
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(self.filter.clone()))
            .add_directive("h2=error".parse()?)
            .add_directive("hyper=error".parse()?)
            .add_directive("tonic=error".parse()?)
            .add_directive("reqwest=error".parse()?);

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer())
            .with(OpenTelemetryTracingBridge::new(&self.logger))
            .init();

        Ok(())
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn test_init(&self) {
        global::set_tracer_provider(self.tracer.clone());
        global::set_meter_provider(self.meter.clone());

        let filter = EnvFilter::default()
            .add_directive("debug".parse().unwrap())
            .add_directive("blockvisor_api::config::provider=info".parse().unwrap())
            .add_directive("h2=info".parse().unwrap())
            .add_directive("opentelemetry_sdk=info".parse().unwrap())
            .add_directive("tokio_postgres=info".parse().unwrap())
            .add_directive("tower_http=off".parse().unwrap());

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer())
            .with(OpenTelemetryTracingBridge::new(&self.logger))
            .init();
    }

    pub fn shutdown(&self) -> Result<(), Error> {
        self.tracer.force_flush()?;
        self.tracer.shutdown()?;

        self.meter.force_flush()?;
        self.meter.shutdown()?;

        self.logger.force_flush()?;
        self.logger.shutdown()?;

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub environment: Environment,
    pub filter: String,
    pub opentelemetry: OpentelemetryConfig,
}

impl Config {
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
