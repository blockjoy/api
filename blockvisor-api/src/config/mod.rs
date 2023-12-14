pub mod chargebee;
pub mod cloudflare;
pub mod database;
pub mod email;
pub mod grpc;
pub mod log;
pub mod mqtt;
pub mod slack;
pub mod storage;
pub mod token;

pub mod context;
pub use context::Context;

pub mod provider;
pub use provider::Provider;

use std::any::type_name;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::{env, fmt};

use derive_more::{Deref, From};
use displaydoc::Display;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

const CONFIG_FILE_ENV: &str = "CONFIG_FILE";
const CONFIG_FILE_DEFAULT: &str = "config.toml";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse Chargebee Config: {0}
    Chargebee(chargebee::Error),
    /// Failed to convert to chrono::Duration: {0}
    ChronoDuration(chrono::OutOfRangeError),
    /// Failed to parse Cloudflare Config: {0}
    Cloudflare(cloudflare::Error),
    /// Failed to parse database Config: {0}
    Database(database::Error),
    /// Failed to parse email Config: {0}
    Email(email::Error),
    /// Failed to parse gRPC Config: {0}
    Grpc(grpc::Error),
    /// Failed to parse HumanTime: {0}
    HumanTime(serde_json::Error),
    /// Failed to parse Log Config: {0}
    Log(log::Error),
    /// Failed to parse MQTT Config: {0}
    Mqtt(mqtt::Error),
    /// No config file at path: {0}
    NoConfigFile(String),
    /// Failed to create Provider: {0}
    Provider(provider::Error),
    /// Failed to parse Redacted<{0}>: {1}
    Redacted(
        &'static str,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    ),
    /// Failed to parse storage Config: {0}
    Storage(storage::Error),
    /// Failed to parse token Config: {0}
    Token(token::Error),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub chargebee: Arc<chargebee::Config>,
    pub cloudflare: Arc<cloudflare::Config>,
    pub database: Arc<database::Config>,
    pub email: Arc<email::Config>,
    pub grpc: Arc<grpc::Config>,
    pub log: Arc<log::Config>,
    pub mqtt: Arc<mqtt::Config>,
    pub storage: Arc<storage::Config>,
    pub token: Arc<token::Config>,
}

impl Config {
    pub fn new() -> Result<Self, Error> {
        let provider = if let Ok(file) = env::var(CONFIG_FILE_ENV) {
            let path = Path::new(&file);
            if path.exists() {
                Provider::new(Some(path)).map_err(Error::Provider)
            } else {
                Err(Error::NoConfigFile(file))
            }
        } else {
            let path = Path::new(CONFIG_FILE_DEFAULT);
            let toml = if path.exists() { Some(path) } else { None };
            Provider::new(toml).map_err(Error::Provider)
        }?;

        TryInto::try_into(&provider)
    }

    pub fn from_toml<P: AsRef<std::path::Path>>(toml: P) -> Result<Self, Error> {
        let provider = Provider::new(Some(toml)).map_err(Error::Provider)?;
        TryInto::try_into(&provider)
    }

    pub fn from_default_toml() -> Result<Self, Error> {
        env::var(CONFIG_FILE_ENV)
            .map_or_else(|_| Self::from_toml(CONFIG_FILE_DEFAULT), Self::from_toml)
    }
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let chargebee = chargebee::Config::try_from(provider)
            .map(Arc::new)
            .map_err(Error::Chargebee)?;
        let cloudflare = cloudflare::Config::try_from(provider)
            .map(Arc::new)
            .map_err(Error::Cloudflare)?;
        let database = database::Config::try_from(provider)
            .map(Arc::new)
            .map_err(Error::Database)?;
        let email = email::Config::try_from(provider)
            .map(Arc::new)
            .map_err(Error::Email)?;
        let grpc = grpc::Config::try_from(provider)
            .map(Arc::new)
            .map_err(Error::Grpc)?;
        let log = log::Config::try_from(provider)
            .map(Arc::new)
            .map_err(Error::Log)?;
        let mqtt = mqtt::Config::try_from(provider)
            .map(Arc::new)
            .map_err(Error::Mqtt)?;
        let storage = storage::Config::try_from(provider)
            .map(Arc::new)
            .map_err(Error::Storage)?;
        let token = token::Config::try_from(provider)
            .map(Arc::new)
            .map_err(Error::Token)?;

        Ok(Config {
            chargebee,
            cloudflare,
            database,
            email,
            grpc,
            log,
            mqtt,
            storage,
            token,
        })
    }
}

/// A type wrapper for sensitive config variables with a redacted Debug implementation.
#[derive(Deref, Deserialize, From)]
pub struct Redacted<T>(T);

impl<T> fmt::Debug for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<Redacted {}>", type_name::<T>())
    }
}

impl<T, E> FromStr for Redacted<T>
where
    T: FromStr<Err = E>,
    E: std::error::Error + Send + Sync + 'static,
{
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<T>()
            .map(Self)
            .map_err(|err| Error::Redacted(type_name::<T>(), Box::new(err)))
    }
}

impl<T: Default> Default for Redacted<T> {
    fn default() -> Self {
        Redacted(T::default())
    }
}

/// Convenience wrapper around a `Duration` for easier parsing.
#[derive(Clone, Copy, Debug, Deref, Deserialize, From)]
pub struct HumanTime(#[serde(with = "humantime_serde")] Duration);

impl FromStr for HumanTime {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_value(Value::String(s.into())).map_err(Error::HumanTime)
    }
}

impl TryFrom<HumanTime> for chrono::Duration {
    type Error = Error;

    fn try_from(duration: HumanTime) -> Result<Self, Self::Error> {
        chrono::Duration::from_std(*duration).map_err(Error::ChronoDuration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacted_type() {
        let redacted: Redacted<String> = "secret".parse().unwrap();
        assert_eq!(format!("{redacted:?}"), "<Redacted alloc::string::String>");
    }

    #[test]
    fn human_time_parsing() {
        let time = "30s".parse::<HumanTime>().unwrap();
        let secs = 30;
        assert_eq!(*time, Duration::from_secs(secs));

        let time = "3days 2hours 4min 59s".parse::<HumanTime>().unwrap();
        let secs = 3 * 24 * 60 * 60 + 2 * 60 * 60 + 4 * 60 + 59;
        assert_eq!(*time, Duration::from_secs(secs));
    }
}
