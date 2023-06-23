use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;
use url::{self, Url};

use super::provider::{self, Provider};

const URL_VAR: &str = "KEY_SERVICE_URL";
const URL_ENTRY: &str = "key_service.url";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse {URL_ENTRY:?}: {0}
    Url(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub url: Url,
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let url = provider.read(URL_VAR, URL_ENTRY).map_err(Error::Url)?;

        Ok(Config { url })
    }
}
