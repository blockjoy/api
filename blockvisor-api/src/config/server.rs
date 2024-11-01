use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;

use super::provider::{self, Provider};

const IP_VAR: &str = "BIND_IP";
const IP_ENTRY: &str = "server.ip";
const IP_DEFAULT: Ipv4Addr = Ipv4Addr::UNSPECIFIED;
const PORT_VAR: &str = "PORT";
const PORT_ENTRY: &str = "server.port";
const PORT_DEFAULT: u16 = 8080;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse {IP_ENTRY:?}: {0}
    Ip(provider::Error),
    /// Failed to parse {PORT_ENTRY:?}: {0}
    Port(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub ip: IpAddr,
    pub port: u16,
}

impl Config {
    pub const fn addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip, self.port)
    }
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let ip = provider
            .read_or(IP_DEFAULT, IP_VAR, IP_ENTRY)
            .map_err(Error::Ip)?;
        let port = provider
            .read_or(PORT_DEFAULT, PORT_VAR, PORT_ENTRY)
            .map_err(Error::Port)?;

        Ok(Config { ip, port })
    }
}
