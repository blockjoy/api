use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;
use tonic::metadata::errors;
use url::Url;

use super::provider::{self, Provider};

const DIR_CHAINS_PREFIX_VAR: &str = "DIR_CHAINS_PREFIX";
const DIR_CHAINS_PREFIX_ENTRY: &str = "cookbook.prefix";
const R2_BUCKET_VAR: &str = "R2_BUCKET";
const R2_BUCKET_ENTRY: &str = "cookbook.r2_bucket";
const R2_URL_VAR: &str = "R2_URL";
const R2_URL_ENTRY: &str = "cookbook.url";
const PRESIGNED_URL_EXPIRATION_SECS_VAR: &str = "PRESIGNED_URL_EXPIRATION_SECS";
const PRESIGNED_URL_EXPIRATION_SECS_ENTRY: &str = "cookbook.expiration";
const REGION_VAR: &str = "AWS_REGION";
const REGION_ENTRY: &str = "cookbook.aws_region";
const AWS_ACCESS_KEY_ID_VAR: &str = "AWS_ACCESS_KEY_ID";
const AWS_ACCESS_KEY_ID_ENTRY: &str = "cookbook.aws_access_key_id";
const AWS_SECRET_ACCESS_KEY_VAR: &str = "AWS_SECRET_ACCESS_KEY";
const AWS_SECRET_ACCESS_KEY_ENTRY: &str = "cookbook.aws_secret_access_key";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create authorization header: {0}
    AuthHeader(errors::InvalidMetadataValue),
    /// Failed to read {DIR_CHAINS_PREFIX_VAR:?}: {0}
    ReadPrefix(provider::Error),
    /// Failed to parse {R2_BUCKET_VAR:?}: {0}
    ReadBucket(provider::Error),
    /// Failed to parse {R2_URL_VAR:?}: {0}
    ReadUrl(provider::Error),
    /// Failed to parse {PRESIGNED_URL_EXPIRATION_SECS_VAR:?}: {0}
    ReadExpiration(provider::Error),
    /// Failed to parse {REGION_VAR:?}: {0}
    ReadRegion(provider::Error),
    /// Failed to parse {AWS_ACCESS_KEY_ID_VAR:?}: {0}
    ReadKeyId(provider::Error),
    /// Failed to parse {AWS_SECRET_ACCESS_KEY_ENTRY:?}: {0}
    ReadKey(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub dir_chains_prefix: String,
    pub r2_bucket: String,
    pub r2_url: Url,
    pub presigned_url_expiration: super::HumanTime,
    pub region: String,
    pub key_id: super::Redacted<String>,
    pub key: super::Redacted<String>,
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(Config {
            dir_chains_prefix: provider
                .read(DIR_CHAINS_PREFIX_VAR, DIR_CHAINS_PREFIX_ENTRY)
                .map_err(Error::ReadPrefix)?,
            r2_bucket: provider
                .read(R2_BUCKET_VAR, R2_BUCKET_ENTRY)
                .map_err(Error::ReadBucket)?,
            r2_url: provider
                .read(R2_URL_VAR, R2_URL_ENTRY)
                .map_err(Error::ReadUrl)?,
            presigned_url_expiration: provider
                .read(
                    PRESIGNED_URL_EXPIRATION_SECS_VAR,
                    PRESIGNED_URL_EXPIRATION_SECS_ENTRY,
                )
                .map_err(Error::ReadExpiration)?,
            region: provider
                .read(REGION_VAR, REGION_ENTRY)
                .map_err(Error::ReadRegion)?,
            key_id: provider
                .read(AWS_ACCESS_KEY_ID_VAR, AWS_ACCESS_KEY_ID_ENTRY)
                .map_err(Error::ReadKeyId)?,
            key: provider
                .read(AWS_SECRET_ACCESS_KEY_VAR, AWS_SECRET_ACCESS_KEY_ENTRY)
                .map_err(Error::ReadKey)?,
        })
    }
}
