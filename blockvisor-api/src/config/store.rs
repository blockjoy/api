use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use super::provider::{self, Provider};
use super::{HumanTime, Redacted};

const AWS_ACCESS_KEY_ID_VAR: &str = "AWS_ACCESS_KEY_ID";
const AWS_ACCESS_KEY_ID_ENTRY: &str = "store.aws_access_key_id";
const AWS_SECRET_ACCESS_KEY_VAR: &str = "AWS_SECRET_ACCESS_KEY";
const AWS_SECRET_ACCESS_KEY_ENTRY: &str = "store.aws_secret_access_key";
const DIR_CHAINS_PREFIX_VAR: &str = "DIR_CHAINS_PREFIX";
const DIR_CHAINS_PREFIX_ENTRY: &str = "store.prefix";
const PRESIGNED_URL_EXPIRATION_VAR: &str = "PRESIGNED_URL_EXPIRATION";
const PRESIGNED_URL_EXPIRATION_ENTRY: &str = "store.expiration";
const REGION_VAR: &str = "AWS_REGION";
const REGION_ENTRY: &str = "store.aws_region";
const STORE_URL_VAR: &str = "STORAGE_URL";
const STORE_URL_ENTRY: &str = "store.url";

const ARCHIVE_BUCKET_VAR: &str = "ARCHIVE_BUCKET";
const ARCHIVE_BUCKET_ENTRY: &str = "store.bucket.archive";
const BUNDLE_BUCKET_VAR: &str = "BUNDLE_BUCKET";
const BUNDLE_BUCKET_ENTRY: &str = "store.bucket.bundle";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse BucketConfig: {0}
    Bucket(#[from] BucketError),
    /// Failed to read {PRESIGNED_URL_EXPIRATION_VAR:?}: {0}
    ReadExpiration(provider::Error),
    /// Failed to read {AWS_ACCESS_KEY_ID_VAR:?}: {0}
    ReadKeyId(provider::Error),
    /// Failed to read {AWS_SECRET_ACCESS_KEY_ENTRY:?}: {0}
    ReadKey(provider::Error),
    /// Failed to read {DIR_CHAINS_PREFIX_VAR:?}: {0}
    ReadPrefix(provider::Error),
    /// Failed to read {REGION_VAR:?}: {0}
    ReadRegion(provider::Error),
    /// Failed to parse {STORE_URL_VAR:?}: {0}
    ReadUrl(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub bucket: BucketConfig,
    pub store_url: Url,
    pub key_id: Redacted<String>,
    pub key: Redacted<String>,
    pub region: String,
    pub dir_chains_prefix: String,
    pub presigned_url_expiration: HumanTime,
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(Config {
            bucket: provider.try_into()?,
            store_url: provider
                .read(STORE_URL_VAR, STORE_URL_ENTRY)
                .map_err(Error::ReadUrl)?,
            key_id: provider
                .read(AWS_ACCESS_KEY_ID_VAR, AWS_ACCESS_KEY_ID_ENTRY)
                .map_err(Error::ReadKeyId)?,
            key: provider
                .read(AWS_SECRET_ACCESS_KEY_VAR, AWS_SECRET_ACCESS_KEY_ENTRY)
                .map_err(Error::ReadKey)?,
            region: provider
                .read(REGION_VAR, REGION_ENTRY)
                .map_err(Error::ReadRegion)?,
            dir_chains_prefix: provider
                .read(DIR_CHAINS_PREFIX_VAR, DIR_CHAINS_PREFIX_ENTRY)
                .map_err(Error::ReadPrefix)?,
            presigned_url_expiration: provider
                .read(PRESIGNED_URL_EXPIRATION_VAR, PRESIGNED_URL_EXPIRATION_ENTRY)
                .map_err(Error::ReadExpiration)?,
        })
    }
}

#[derive(Debug, Display, Error)]
pub enum BucketError {
    /// Failed to read {ARCHIVE_BUCKET_VAR:?}: {0}
    ReadArchive(provider::Error),
    /// Failed to read {BUNDLE_BUCKET_VAR:?}: {0}
    ReadBundle(provider::Error),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BucketConfig {
    pub archive: String,
    pub bundle: String,
}

impl TryFrom<&Provider> for BucketConfig {
    type Error = BucketError;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(BucketConfig {
            archive: provider
                .read(ARCHIVE_BUCKET_VAR, ARCHIVE_BUCKET_ENTRY)
                .map_err(BucketError::ReadArchive)?,
            bundle: provider
                .read(BUNDLE_BUCKET_VAR, BUNDLE_BUCKET_ENTRY)
                .map_err(BucketError::ReadBundle)?,
        })
    }
}
