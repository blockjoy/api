use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;

use super::provider::{self, Provider};

const REQUEST_CONCURRENCY_LIMIT_VAR: &str = "REQUEST_CONCURRENCY_LIMIT";
const REQUEST_CONCURRENCY_LIMIT_ENTRY: &str = "grpc.request_concurrency_limit";
const REQUEST_CONCURRENCY_LIMIT_DEFAULT: usize = 32;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse {REQUEST_CONCURRENCY_LIMIT_ENTRY:?}: {0}
    RequestConcurrencyLimit(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub request_concurrency_limit: usize,
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let request_concurrency_limit = provider
            .read_or(
                REQUEST_CONCURRENCY_LIMIT_DEFAULT,
                REQUEST_CONCURRENCY_LIMIT_VAR,
                REQUEST_CONCURRENCY_LIMIT_ENTRY,
            )
            .map_err(Error::RequestConcurrencyLimit)?;

        Ok(Config {
            request_concurrency_limit,
        })
    }
}
