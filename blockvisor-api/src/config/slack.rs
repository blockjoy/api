use derive_more::{Deref, FromStr};
use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;

use super::provider::{self, Provider};
use super::Redacted;

const SLACK_TOKEN_VAR: &str = "SLACK_TOKEN";
const SLACK_TOKEN_ENTRY: &str = "slack.token";
const SLACK_URL_VAR: &str = "SLACK_URL";
const SLACK_URL_ENTRY: &str = "slack.url";
const SLACK_CHANNEL_ID_VAR: &str = "SLACK_CHANNEL_ID";
const SLACK_CHANNEL_ID_ENTRY: &str = "slack.channel_id";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to find {SLACK_TOKEN_ENTRY:?}: {0}
    TokenMissing(provider::Error),
    /// Failed to find {SLACK_URL_ENTRY:?}: {0}
    UrlMissing(provider::Error),
    /// Failed to find {SLACK_CHANNEL_ID_ENTRY:?}: {0}
    ChannelIdMissing(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub token: Token,
    pub url: String,
    pub channel_id: String,
}

#[derive(Debug, FromStr, Deserialize, Deref)]
pub struct Token(Redacted<String>);

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let token = provider
            .read(SLACK_TOKEN_VAR, SLACK_TOKEN_ENTRY)
            .map_err(Error::TokenMissing)?;
        let url = provider
            .read(SLACK_URL_VAR, SLACK_URL_ENTRY)
            .map_err(Error::UrlMissing)?;
        let channel_id = provider
            .read(SLACK_CHANNEL_ID_VAR, SLACK_CHANNEL_ID_ENTRY)
            .map_err(Error::ChannelIdMissing)?;

        Ok(Config {
            token,
            url,
            channel_id,
        })
    }
}
