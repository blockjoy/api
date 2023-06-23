use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;
use tonic::metadata::errors;
use url::Url;

use super::provider::{self, Provider};

const DIR_CHAINS_PREFIX_VAR: &str = "DIR_CHAINS_PREFIX";
const DIR_CHAINS_PREFIX_ENTRY: &str = "cookbook.prefix";
const R2_ROOT_VAR: &str = "R2_ROOT";
const R2_ROOT_ENTRY: &str = "cookbook.root";
const R2_URL_VAR: &str = "R2_URL";
const R2_URL_ENTRY: &str = "cookbook.url";
const PRESIGNED_URL_EXPIRATION_SECS_VAR: &str = "PRESIGNED_URL_EXPIRATION_SECS";
const PRESIGNED_URL_EXPIRATION_SECS_ENTRY: &str = "cookbook.expiration";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create authorization header: {0}
    AuthHeader(errors::InvalidMetadataValue),
    /// Failed to read {DIR_CHAINS_PREFIX_VAR:?}: {0}
    ReadPrefix(provider::Error),
    /// Failed to parse {R2_URL_VAR:?}: {0}
    ReadRoot(provider::Error),
    /// Failed to parse {R2_URL_VAR:?}: {0}
    ReadUrl(provider::Error),
    /// Failed to parse {R2_URL_VAR:?}: {0}
    ReadExpiration(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub dir_chains_prefix: String,
    pub r2_root: String,
    pub r2_url: Url,
    pub presigned_url_expiration: super::HumanTime,
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(Config {
            dir_chains_prefix: provider
                .read(DIR_CHAINS_PREFIX_VAR, DIR_CHAINS_PREFIX_ENTRY)
                .map_err(Error::ReadPrefix)?,
            r2_root: provider
                .read(R2_ROOT_VAR, R2_ROOT_ENTRY)
                .map_err(Error::ReadRoot)?,
            r2_url: provider
                .read(R2_URL_VAR, R2_URL_ENTRY)
                .map_err(Error::ReadUrl)?,
            presigned_url_expiration: provider
                .read(
                    PRESIGNED_URL_EXPIRATION_SECS_VAR,
                    PRESIGNED_URL_EXPIRATION_SECS_ENTRY,
                )
                .map_err(Error::ReadExpiration)?,
        })
    }
}
