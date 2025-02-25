pub mod client;
pub use client::Client;

pub mod manifest;

pub mod secret;
pub use secret::Secret;

use std::sync::Arc;
use std::time::Duration;

use aws_sdk_s3::config::{
    Credentials, Region, RequestChecksumCalculation, ResponseChecksumValidation,
};
use derive_more::{Deref, Display, Into};
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use url::Url;

use crate::config::store::{BucketConfig, Config};
use crate::grpc::{Status, api};
use crate::util::LOWER_KEBAB_CASE;

use self::manifest::{ArchiveChunk, DownloadManifest, ManifestBody, ManifestHeader, UploadSlot};

pub const CREDENTIALS: &str = "api-credentials-provider";
pub const BUNDLE_FILE: &str = "bvd-bundle.tgz";
pub const MANIFEST_BODY: &str = "manifest-body.json";
pub const MANIFEST_HEADER: &str = "manifest-header.json";

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Store client error: {0}
    Client(#[from] client::Error),
    /// Storage manifest error: {0}
    Manifest(#[from] manifest::Error),
    /// Missing chunk index: {0}
    MissingChunk(usize),
    /// Missing `ManifestBody` for `StoreKey` {0}
    MissingManifestBody(StoreKey),
    /// Missing `ManifestHeader` for `StoreKey` {0}
    MissingManifestHeader(StoreKey),
    /// No data versions found.
    NoDataVersion,
    /// Failed to parse `ManifestBody` for `StoreKey` {0}: {1}
    ParseManifestBody(StoreKey, serde_json::Error),
    /// Failed to parse `ManifestHeader` for `StoreKey` {0}: {1}
    ParseManifestHeader(StoreKey, serde_json::Error),
    /// Failed to read `ManifestBody` for `StoreKey` {0}: {1}
    ReadManifestBody(StoreKey, client::Error),
    /// Failed to read `ManifestHeader` for `StoreKey` {0}: {1}
    ReadManifestHeader(StoreKey, client::Error),
    /// Failed to serialize ManifestBody: {0}
    SerializeBody(serde_json::Error),
    /// Failed to serialize ManifestHeader: {0}
    SerializeHeader(serde_json::Error),
    /// StoreKey is not lower-kebab-case: {0}
    StoreKeyChars(String),
    /// StoreKey length `{0}` must be at least 6 characters.
    StoreKeyLen(usize),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Client(client::Error::MissingKey(_, _)) | NoDataVersion => {
                Status::not_found("Store not found.")
            }
            Client(_)
            | Manifest(_)
            | ParseManifestHeader(_, _)
            | ParseManifestBody(_, _)
            | ReadManifestHeader(_, _)
            | ReadManifestBody(_, _)
            | SerializeBody(_)
            | SerializeHeader(_) => Status::internal("Internal error."),
            MissingManifestBody(_) | MissingManifestHeader(_) => {
                Status::not_found("Manifest not found.")
            }
            MissingChunk(_) => Status::failed_precondition("Unknown chunk index."),
            StoreKeyChars(_) | StoreKeyLen(_) => Status::invalid_argument("store_key"),
        }
    }
}

#[derive(Clone, Debug, Display, PartialEq, Eq, DieselNewType, Deref, Into)]
pub struct StoreKey(String);

impl StoreKey {
    pub fn new(id: String) -> Result<Self, Error> {
        if id.len() < 6 {
            Err(Error::StoreKeyLen(id.len()))
        } else if !id.chars().all(|c| LOWER_KEBAB_CASE.contains(c)) {
            Err(Error::StoreKeyChars(id))
        } else {
            Ok(StoreKey(id))
        }
    }
}

pub struct Store {
    pub client: Arc<dyn Client>,
    pub bucket: BucketConfig,
    pub prefix: String,
    pub expiration: Duration,
}

impl Store {
    pub fn new<C>(client: C, config: &Config) -> Self
    where
        C: Client + 'static,
    {
        Store {
            client: Arc::new(client),
            bucket: config.bucket.clone(),
            prefix: config.dir_chains_prefix.clone(),
            expiration: *config.presigned_url_expiration,
        }
    }

    pub fn new_s3(config: &Config) -> Self {
        let credentials = Credentials::new(&*config.key_id, &*config.key, None, None, CREDENTIALS);
        let s3_config = aws_sdk_s3::Config::builder()
            .endpoint_url(config.store_url.to_string())
            .region(Region::new(config.region.clone()))
            .credentials_provider(credentials)
            .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
            .response_checksum_validation(ResponseChecksumValidation::WhenRequired);

        let client = aws_sdk_s3::Client::from_conf(s3_config.build());
        Self::new(client, config)
    }

    /// Return a descending order list of data versions for a `StoreKey`.
    async fn data_versions(&self, store_key: &StoreKey) -> Result<Vec<u64>, Error> {
        let path = format!("{store_key}/");
        let mut versions: Vec<_> = self
            .client
            .list(&self.bucket.archive, &path)
            .await?
            .iter()
            .filter_map(|path| {
                path.trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .and_then(|version| version.parse::<u64>().ok())
            })
            .collect();

        versions.sort_by(|a, b| b.cmp(a));
        Ok(versions)
    }

    /// Fetch and parse a download manifest header.
    ///
    /// If `data_version` is None then it uses the latest data version.
    pub async fn download_manifest_header(
        &self,
        store_key: &StoreKey,
        data_version: Option<u64>,
    ) -> Result<(ManifestHeader, u64), Error> {
        let data_version = if let Some(version) = data_version {
            version
        } else {
            let mut versions = self.data_versions(store_key).await?;
            versions.pop().ok_or(Error::NoDataVersion)?
        };

        let key = format!("{store_key}/{data_version}/{MANIFEST_HEADER}");
        match self.client.read_key(&self.bucket.archive, &key).await {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(header) => Ok((header, data_version)),
                Err(err) => Err(Error::ParseManifestHeader(store_key.clone(), err)),
            },
            Err(client::Error::MissingKey(_, _)) => {
                Err(Error::MissingManifestHeader(store_key.clone()))
            }
            Err(err) => Err(Error::ReadManifestHeader(store_key.clone(), err)),
        }
    }

    /// Fetch and parse a download manifest body.
    ///
    /// If `data_version` is None then it uses the latest data version.
    async fn download_manifest_body(
        &self,
        store_key: &StoreKey,
        data_version: Option<u64>,
    ) -> Result<(ManifestBody, u64), Error> {
        let data_version = if let Some(version) = data_version {
            version
        } else {
            let mut versions = self.data_versions(store_key).await?;
            versions.pop().ok_or(Error::NoDataVersion)?
        };

        let key = format!("{store_key}/{data_version}/{MANIFEST_BODY}");
        match self.client.read_key(&self.bucket.archive, &key).await {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(body) => Ok((body, data_version)),
                Err(err) => Err(Error::ParseManifestBody(store_key.clone(), err)),
            },
            Err(client::Error::MissingKey(_, _)) => {
                Err(Error::MissingManifestBody(store_key.clone()))
            }
            Err(err) => Err(Error::ReadManifestBody(store_key.clone(), err)),
        }
    }

    /// Regenerate the download URLs for the requested `DownloadManifest` chunks.
    pub async fn refresh_download_manifest(
        &self,
        store_key: &StoreKey,
        data_version: u64,
        chunk_indexes: &[usize],
    ) -> Result<Vec<ArchiveChunk>, Error> {
        let (manifest, _) = self
            .download_manifest_body(store_key, Some(data_version))
            .await?;
        let expires = Duration::from_secs(self.expiration.as_secs());

        let mut chunks = Vec::with_capacity(chunk_indexes.len());
        for &index in chunk_indexes {
            let mut chunk = manifest
                .chunks
                .get(index)
                .ok_or(Error::MissingChunk(index))?
                .clone();
            chunk.index = Some(index);
            chunk.url = self
                .client
                .download_url(&self.bucket.archive, &chunk.key, expires)
                .await
                .map(Some)?;
            chunks.push(chunk);
        }

        Ok(chunks)
    }

    pub async fn save_download_manifest(
        &self,
        store_key: &StoreKey,
        manifest: DownloadManifest,
    ) -> Result<(), Error> {
        let mut versions = self.data_versions(store_key).await?;
        let data_version = versions.pop().unwrap_or_default();

        let header_key = format!("{store_key}/{data_version}/{MANIFEST_HEADER}");
        let header: ManifestHeader = (&manifest).try_into()?;
        let header_data = serde_json::to_vec(&header).map_err(Error::SerializeHeader)?;
        self.client
            .write_key(&self.bucket.archive, &header_key, header_data)
            .await?;

        let body_key = format!("{store_key}/{data_version}/{MANIFEST_BODY}");
        let body: ManifestBody = manifest.into();
        let body_data = serde_json::to_vec(&body).map_err(Error::SerializeBody)?;
        self.client
            .write_key(&self.bucket.archive, &body_key, body_data)
            .await
            .map_err(Into::into)
    }

    pub async fn upload_slots(
        &self,
        store_key: &StoreKey,
        data_version: Option<u64>,
        slot_indexes: &[usize],
        expires: Duration,
    ) -> Result<(Vec<UploadSlot>, u64), Error> {
        let data_version = if let Some(version) = data_version {
            version
        } else {
            let mut versions = self.data_versions(store_key).await?;
            versions.pop().unwrap_or_default() + 1
        };

        let mut slots = Vec::with_capacity(slot_indexes.len());
        for &index in slot_indexes {
            let key = format!("{store_key}/{data_version}/data.part_{index}");
            let url = self
                .client
                .upload_url(&self.bucket.archive, &key, expires)
                .await?;
            slots.push(UploadSlot { index, key, url });
        }

        Ok((slots, data_version))
    }

    pub async fn list_bundles(&self) -> Result<Vec<api::BundleIdentifier>, Error> {
        let keys = self.client.list_recursive(&self.bucket.bundle, "").await?;
        Ok(keys
            .iter()
            .filter_map(api::BundleIdentifier::maybe_from_key)
            .collect())
    }

    pub async fn download_bundle(&self, version: &str) -> Result<Url, Error> {
        let key = format!("{version}/{BUNDLE_FILE}");
        self.client
            .download_url(&self.bucket.bundle, &key, self.expiration)
            .await
            .map_err(Into::into)
    }
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use mockito::{Matcher, Server, ServerGuard};

    use super::client::Error;
    use super::*;

    pub struct TestStore {
        mock: ServerGuard,
    }

    impl TestStore {
        pub async fn new() -> Self {
            let mut mock = Server::new_async().await;
            mock.mock("POST", Matcher::Regex(r"^/*".to_string()))
                .with_status(200)
                .with_body("{\"data\":\"id\"}")
                .create_async()
                .await;

            TestStore { mock }
        }

        pub fn mock_store(&self) -> Store {
            let client = MockClient {};
            let config = Config {
                bucket: BucketConfig {
                    bundle: "bundle".to_string(),
                    archive: "archive".to_string(),
                },
                store_url: self.mock.url().parse().unwrap(),
                key_id: "key_id".parse().unwrap(),
                key: "key".parse().unwrap(),
                region: "eu-west-3".to_string(),
                dir_chains_prefix: "prefix".to_string(),
                presigned_url_expiration: "1d".parse().unwrap(),
            };

            Store::new(client, &config)
        }
    }

    struct MockClient {}

    #[tonic::async_trait]
    impl Client for MockClient {
        async fn list(&self, _: &str, _: &str) -> Result<Vec<String>, Error> {
            unimplemented!()
        }

        async fn list_recursive(&self, _: &str, _: &str) -> Result<Vec<String>, Error> {
            unimplemented!()
        }

        async fn read_key(&self, _: &str, _: &str) -> Result<Vec<u8>, Error> {
            unimplemented!()
        }

        async fn write_key(&self, _: &str, _: &str, _: Vec<u8>) -> Result<(), Error> {
            unimplemented!()
        }

        async fn download_url(&self, _: &str, _: &str, _: Duration) -> Result<Url, Error> {
            unimplemented!()
        }

        async fn upload_url(&self, _: &str, _: &str, _: Duration) -> Result<Url, Error> {
            unimplemented!()
        }
    }
}
