use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;

use super::provider;
use super::Redacted;

const STRIPE_SECRET_VAR: &str = "STRIPE_SECRET";
const STRIPE_SECRET_ENTRY: &str = "stripe.secret";

const STRIPE_URL_VAR: &str = "STRIPE_URL";
const STRIPE_URL_ENTRY: &str = "stripe.url";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to read {STRIPE_SECRET_VAR:?}: {0}
    ReadSecret(provider::Error),
    /// Failed to read {STRIPE_URL_VAR:?}: {0}
    ReadUrl(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub secret: Redacted<String>,
    pub base_url: String,
}

impl TryFrom<&provider::Provider> for Config {
    type Error = Error;

    fn try_from(provider: &provider::Provider) -> Result<Self, Self::Error> {
        Ok(Config {
            secret: provider
                .read(STRIPE_SECRET_VAR, STRIPE_SECRET_ENTRY)
                .map_err(Error::ReadSecret)?,
            base_url: provider
                .read(STRIPE_URL_VAR, STRIPE_URL_ENTRY)
                .map_err(Error::ReadUrl)?,
        })
    }
}
