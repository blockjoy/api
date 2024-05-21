use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;

use super::provider;
use super::Redacted;

const STRIPE_SECRET_VAR: &str = "STRIPE_SECRET";
const STRIPE_SECRET_ENTRY: &str = "stripe.secret";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to read {STRIPE_SECRET_VAR:?}: {0}
    ReadSecret(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub secret: Redacted<String>,
}

impl TryFrom<&provider::Provider> for Config {
    type Error = Error;

    fn try_from(provider: &provider::Provider) -> Result<Self, Self::Error> {
        Ok(Config {
            secret: provider
                .read(STRIPE_SECRET_VAR, STRIPE_SECRET_ENTRY)
                .map_err(Error::ReadSecret)?,
        })
    }
}
