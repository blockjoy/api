use std::fmt;

use derive_more::{Deref, FromStr};
use displaydoc::Display;
use rumqttc::MqttOptions;
use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;

use super::provider::{self, Provider};
use super::Redacted;

const SERVER_ADDRESS_VAR: &str = "MQTT_SERVER_ADDRESS";
const SERVER_ADDRESS_ENTRY: &str = "mqtt.server_address";
const SERVER_PORT_VAR: &str = "MQTT_SERVER_PORT";
const SERVER_PORT_ENTRY: &str = "mqtt.server_port";

const USERNAME_VAR: &str = "MQTT_USERNAME";
const USERNAME_ENTRY: &str = "mqtt.username";
const PASSWORD_VAR: &str = "MQTT_PASSWORD";
const PASSWORD_ENTRY: &str = "mqtt.password";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse {PASSWORD_ENTRY:?}: {0}
    ParsePassword(provider::Error),
    /// Failed to parse {SERVER_ADDRESS_ENTRY:?}: {0}
    ParseServerAddress(provider::Error),
    /// Failed to parse {SERVER_PORT_ENTRY:?}: {0}
    ParseServerPort(provider::Error),
    /// Failed to parse {USERNAME_ENTRY:?}: {0}
    ParseUsername(provider::Error),
}

#[derive(Debug, Deref, Deserialize, FromStr)]
#[deref(forward)]
pub struct Password(Redacted<String>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server_address: String,
    pub server_port: u16,
    pub username: String,
    pub password: Password,
}

impl Config {
    pub fn new_options(&self) -> MqttOptions {
        let client_id = format!("blockvisor-api-{}", Uuid::new_v4());
        let mut options = MqttOptions::new(client_id, &self.server_address, self.server_port);
        options.set_credentials(&self.username, &*self.password);
        options
    }

    pub fn notification_url(&self) -> String {
        let scheme: Scheme = self.into();
        format!("{scheme}://{}:{}", self.server_address, self.server_port)
    }
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(Config {
            server_address: provider
                .read(SERVER_ADDRESS_VAR, SERVER_ADDRESS_ENTRY)
                .map_err(Error::ParseServerAddress)?,
            server_port: provider
                .read(SERVER_PORT_VAR, SERVER_PORT_ENTRY)
                .map_err(Error::ParseServerPort)?,
            username: provider
                .read(USERNAME_VAR, USERNAME_ENTRY)
                .map_err(Error::ParseUsername)?,
            password: provider
                .read(PASSWORD_VAR, PASSWORD_ENTRY)
                .map_err(Error::ParsePassword)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub enum Scheme {
    #[serde(rename = "mqtt")]
    Tcp,
    #[serde(rename = "mqtts")]
    Ssl,
    #[serde(rename = "ws")]
    Websocket,
    #[serde(rename = "wss")]
    WebsocketSecure,
}

impl From<&Config> for Scheme {
    fn from(config: &Config) -> Scheme {
        match config.server_port {
            8083 => Scheme::Websocket,
            8084 => Scheme::WebsocketSecure,
            8883 => Scheme::Ssl,
            _ => Scheme::Tcp,
        }
    }
}

impl From<Scheme> for &'static str {
    fn from(scheme: Scheme) -> Self {
        match scheme {
            Scheme::Tcp => "mqtt",
            Scheme::Ssl => "mqtts",
            Scheme::Websocket => "ws",
            Scheme::WebsocketSecure => "wss",
        }
    }
}

impl fmt::Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Into::<&'static str>::into(*self))
    }
}
