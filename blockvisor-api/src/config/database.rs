use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;
use url::{self, Url};

use super::provider::{self, Provider};
use super::HumanTime;

const URL_VAR: &str = "DATABASE_URL";
const URL_ENTRY: &str = "database.url";
const MAX_CONNS_VAR: &str = "DB_MAX_CONN";
const MAX_CONNS_ENTRY: &str = "database.max_conns";
const MAX_CONNS_DEFAULT: u32 = 10;
const MIN_CONNS_VAR: &str = "DB_MIN_CONN";
const MIN_CONNS_ENTRY: &str = "database.min_conns";
const MIN_CONNS_DEFAULT: u32 = 2;
const MAX_LIFETIME_VAR: &str = "DB_MAX_LIFETIME";
const MAX_LIFETIME_ENTRY: &str = "database.max_lifetime";
const MAX_LIFETIME_DEFAULT: &str = "1d";
const IDLE_TIMEOUT_VAR: &str = "DB_IDLE_TIMEOUT";
const IDLE_TIMEOUT_ENTRY: &str = "database.idle_timeout";
const IDLE_TIMEOUT_DEFAULT: &str = "2m";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse {IDLE_TIMEOUT_ENTRY:?}: {0}
    IdleTimeout(provider::Error),
    /// Failed to parse {MAX_CONNS_ENTRY:?}: {0}
    MaxConns(provider::Error),
    /// Failed to parse {MAX_LIFETIME_ENTRY:?}: {0}
    MaxLifetime(provider::Error),
    /// Failed to parse {MIN_CONNS_ENTRY:?}: {0}
    MinConns(provider::Error),
    /// Failed to parse {URL_ENTRY:?}: {0}
    Url(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub url: Url,
    pub max_conns: u32,
    pub min_conns: u32,
    pub max_lifetime: HumanTime,
    pub idle_timeout: HumanTime,
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let url = provider.read(URL_VAR, URL_ENTRY).map_err(Error::Url)?;
        let max_conns = provider
            .read_or(MAX_CONNS_DEFAULT, MAX_CONNS_VAR, MAX_CONNS_ENTRY)
            .map_err(Error::MaxConns)?;
        let min_conns = provider
            .read_or(MIN_CONNS_DEFAULT, MIN_CONNS_VAR, MIN_CONNS_ENTRY)
            .map_err(Error::MinConns)?;
        let max_lifetime = provider
            .read_or_else(
                || MAX_LIFETIME_DEFAULT.parse::<HumanTime>(),
                MAX_LIFETIME_VAR,
                MAX_LIFETIME_ENTRY,
            )
            .map_err(Error::MaxLifetime)?;
        let idle_timeout = provider
            .read_or_else(
                || IDLE_TIMEOUT_DEFAULT.parse::<HumanTime>(),
                IDLE_TIMEOUT_VAR,
                IDLE_TIMEOUT_ENTRY,
            )
            .map_err(Error::IdleTimeout)?;

        Ok(Config {
            url,
            max_conns,
            min_conns,
            max_lifetime,
            idle_timeout,
        })
    }
}
