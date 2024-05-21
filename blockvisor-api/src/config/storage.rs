use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use super::provider::{self, Provider};
use super::{HumanTime, Redacted};

const AWS_ACCESS_KEY_ID_VAR: &str = "AWS_ACCESS_KEY_ID";
const AWS_ACCESS_KEY_ID_ENTRY: &str = "storage.aws_access_key_id";
const AWS_SECRET_ACCESS_KEY_VAR: &str = "AWS_SECRET_ACCESS_KEY";
const AWS_SECRET_ACCESS_KEY_ENTRY: &str = "storage.aws_secret_access_key";
const DIR_CHAINS_PREFIX_VAR: &str = "DIR_CHAINS_PREFIX";
const DIR_CHAINS_PREFIX_ENTRY: &str = "storage.prefix";
const PRESIGNED_URL_EXPIRATION_VAR: &str = "PRESIGNED_URL_EXPIRATION";
const PRESIGNED_URL_EXPIRATION_ENTRY: &str = "storage.expiration";
const REGION_VAR: &str = "AWS_REGION";
const REGION_ENTRY: &str = "storage.aws_region";
const STORAGE_URL_VAR: &str = "STORAGE_URL";
const STORAGE_URL_ENTRY: &str = "storage.url";

const ARCHIVE_BUCKET_VAR: &str = "ARCHIVE_BUCKET";
const ARCHIVE_BUCKET_ENTRY: &str = "storage.bucket.archive";
const BUNDLE_BUCKET_VAR: &str = "BUNDLE_BUCKET";
const BUNDLE_BUCKET_ENTRY: &str = "storage.bucket.bundle";
const COOKBOOK_BUCKET_VAR: &str = "COOKBOOK_BUCKET";
const COOKBOOK_BUCKET_ENTRY: &str = "storage.bucket.cookbook";
const KERNEL_BUCKET_VAR: &str = "KERNEL_BUCKET";
const KERNEL_BUCKET_ENTRY: &str = "storage.bucket.kernel";

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
    /// Failed to parse {STORAGE_URL_VAR:?}: {0}
    ReadUrl(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub bucket: BucketConfig,
    pub storage_url: Url,
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
            storage_url: provider
                .read(STORAGE_URL_VAR, STORAGE_URL_ENTRY)
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
    /// Failed to read {COOKBOOK_BUCKET_VAR:?}: {0}
    ReadCookbook(provider::Error),
    /// Failed to read {BUNDLE_BUCKET_VAR:?}: {0}
    ReadBundle(provider::Error),
    /// Failed to read {KERNEL_BUCKET_VAR:?}: {0}
    ReadKernel(provider::Error),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BucketConfig {
    pub cookbook: String,
    pub bundle: String,
    pub kernel: String,
    pub archive: String,
}

impl TryFrom<&Provider> for BucketConfig {
    type Error = BucketError;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(BucketConfig {
            cookbook: provider
                .read(COOKBOOK_BUCKET_VAR, COOKBOOK_BUCKET_ENTRY)
                .map_err(BucketError::ReadCookbook)?,
            bundle: provider
                .read(BUNDLE_BUCKET_VAR, BUNDLE_BUCKET_ENTRY)
                .map_err(BucketError::ReadBundle)?,
            kernel: provider
                .read(KERNEL_BUCKET_VAR, KERNEL_BUCKET_ENTRY)
                .map_err(BucketError::ReadKernel)?,
            archive: provider
                .read(ARCHIVE_BUCKET_VAR, ARCHIVE_BUCKET_ENTRY)
                .map_err(BucketError::ReadArchive)?,
        })
    }
}
