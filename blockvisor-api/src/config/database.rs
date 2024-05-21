use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;
use url::{self, Url};

use super::provider::{self, Provider};
use super::HumanTime;

const DB_URL_VAR: &str = "DATABASE_URL";
const DB_URL_ENTRY: &str = "database.url";
const DB_BIND_IP_VAR: &str = "BIND_IP";
const DB_BIND_IP_ENTRY: &str = "database.bind_ip";
const DB_BIND_IP_DEFAULT: Ipv4Addr = Ipv4Addr::UNSPECIFIED;
const DB_PORT_VAR: &str = "PORT";
const DB_PORT_ENTRY: &str = "database.port";
const DB_PORT_DEFAULT: u16 = 8080;

const POOL_MAX_CONNS_VAR: &str = "DB_MAX_CONN";
const POOL_MAX_CONNS_ENTRY: &str = "database.pool.max_conns";
const POOL_MAX_CONNS_DEFAULT: u32 = 10;
const POOL_MIN_CONNS_VAR: &str = "DB_MIN_CONN";
const POOL_MIN_CONNS_ENTRY: &str = "database.pool.min_conns";
const POOL_MIN_CONNS_DEFAULT: u32 = 2;
const POOL_MAX_LIFETIME_VAR: &str = "DB_MAX_LIFETIME";
const POOL_MAX_LIFETIME_ENTRY: &str = "database.pool.max_lifetime";
const POOL_MAX_LIFETIME_DEFAULT: &str = "1d";
const POOL_IDLE_TIMEOUT_VAR: &str = "DB_IDLE_TIMEOUT";
const POOL_IDLE_TIMEOUT_ENTRY: &str = "database.pool.idle_timeout";
const POOL_IDLE_TIMEOUT_DEFAULT: &str = "2m";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse {DB_BIND_IP_ENTRY:?}: {0}
    BindIp(provider::Error),
    /// Failed to parse PoolConfig: {0}
    PoolConfig(PoolError),
    /// Failed to parse {DB_PORT_ENTRY:?}: {0}
    Port(provider::Error),
    /// Failed to parse {DB_URL_ENTRY:?}: {0}
    Url(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub url: Url,
    pub bind_ip: IpAddr,
    pub bind_port: u16,
    pub pool: PoolConfig,
}

impl Config {
    pub const fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.bind_ip, self.bind_port)
    }
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let url = provider
            .read(DB_URL_VAR, DB_URL_ENTRY)
            .map_err(Error::Url)?;
        let bind_ip = provider
            .read_or(DB_BIND_IP_DEFAULT, DB_BIND_IP_VAR, DB_BIND_IP_ENTRY)
            .map_err(Error::BindIp)?;
        let bind_port = provider
            .read_or(DB_PORT_DEFAULT, DB_PORT_VAR, DB_PORT_ENTRY)
            .map_err(Error::Port)?;

        let pool: PoolConfig = provider.try_into().map_err(Error::PoolConfig)?;

        Ok(Config {
            url,
            bind_ip,
            bind_port,
            pool,
        })
    }
}

#[derive(Debug, Display, Error)]
pub enum PoolError {
    /// Failed to parse {POOL_MAX_CONNS_ENTRY:?}: {0}
    MaxConns(provider::Error),
    /// Failed to parse {POOL_MIN_CONNS_ENTRY:?}: {0}
    MinConns(provider::Error),
    /// Failed to parse {POOL_MAX_LIFETIME_ENTRY:?}: {0}
    MaxLifetime(provider::Error),
    /// Failed to parse {POOL_IDLE_TIMEOUT_ENTRY:?}: {0}
    IdleTimeout(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolConfig {
    pub max_conns: u32,
    pub min_conns: u32,
    pub max_lifetime: HumanTime,
    pub idle_timeout: HumanTime,
}

impl TryFrom<&Provider> for PoolConfig {
    type Error = PoolError;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let max_conns = provider
            .read_or(
                POOL_MAX_CONNS_DEFAULT,
                POOL_MAX_CONNS_VAR,
                POOL_MAX_CONNS_ENTRY,
            )
            .map_err(PoolError::MaxConns)?;
        let min_conns = provider
            .read_or(
                POOL_MIN_CONNS_DEFAULT,
                POOL_MIN_CONNS_VAR,
                POOL_MIN_CONNS_ENTRY,
            )
            .map_err(PoolError::MinConns)?;
        let max_lifetime = provider
            .read_or_else(
                || POOL_MAX_LIFETIME_DEFAULT.parse::<HumanTime>(),
                POOL_MAX_LIFETIME_VAR,
                POOL_MAX_LIFETIME_ENTRY,
            )
            .map_err(PoolError::MaxLifetime)?;
        let idle_timeout = provider
            .read_or_else(
                || POOL_IDLE_TIMEOUT_DEFAULT.parse::<HumanTime>(),
                POOL_IDLE_TIMEOUT_VAR,
                POOL_IDLE_TIMEOUT_ENTRY,
            )
            .map_err(PoolError::IdleTimeout)?;

        Ok(PoolConfig {
            max_conns,
            min_conns,
            max_lifetime,
            idle_timeout,
        })
    }
}
