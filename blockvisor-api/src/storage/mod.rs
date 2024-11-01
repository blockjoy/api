pub mod client;
pub mod image;
pub mod manifest;
pub mod metadata;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use aws_sdk_s3::config::{Credentials, Region};
use displaydoc::Display;
use rhai::Engine;
use semver::Version;
use thiserror::Error;
use tracing::warn;
use url::Url;

use crate::config::storage::{BucketConfig, Config};
use crate::grpc::{api, Status};
use crate::model::node::NodeType;

use self::client::Client;
use self::image::ImageId;
use self::manifest::{ArchiveChunk, DownloadManifest, ManifestBody, ManifestHeader, UploadSlot};
use self::metadata::BlockchainMetadata;

pub const CREDENTIALS: &str = "blockvisor-api credentials provider";
pub const BABEL_IMAGE_FILE: &str = "blockjoy.gz";
pub const BUNDLE_FILE: &str = "bvd-bundle.tgz";
pub const KERNEL_FILE: &str = "kernel.gz";
pub const MANIFEST_FILE: &str = "manifest.json";
pub const MANIFEST_HEADER: &str = "manifest-header.json";
pub const MANIFEST_BODY: &str = "manifest-body.json";
pub const RHAI_FILE: &str = "babel.rhai";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Storage client error: {0}
    Client(#[from] client::Error),
    /// Failed to find download manifest body for `{0:?}` and network `{1}`.
    FindManifestBody(ImageId, String),
    /// Failed to find download manifest header for `{0:?}` and network `{1}`.
    FindManifestHeader(ImageId, String),
    /// Storage image error: {0}
    Image(#[from] image::Error),
    /// Storage manifest error: {0}
    Manifest(#[from] manifest::Error),
    /// Storage metadata error: {0}
    Metadata(#[from] metadata::Error),
    /// Missing chunk index: {0}
    MissingChunk(usize),
    /// No data versions found.
    NoDataVersion,
    /// Failed to parse DownloadManifest: {0}
    ParseManifest(serde_json::Error),
    /// Failed to parse ManifestBody: {0}
    ParseManifestBody(serde_json::Error),
    /// Failed to parse ManifestHeader: {0}
    ParseManifestHeader(serde_json::Error),
    /// Failed to parse storage bytes as UTF8: {0}
    ParseUtf8(std::string::FromUtf8Error),
    /// Failed to serialize ManifestBody: {0}
    SerializeBody(serde_json::Error),
    /// Failed to serialize ManifestHeader: {0}
    SerializeHeader(serde_json::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Client(client::Error::MissingKey(_, _))
            | FindManifestBody(_, _)
            | FindManifestHeader(_, _)
            | NoDataVersion => Status::not_found("Not found."),
            Metadata(crate::storage::metadata::Error::CompileScript(_, _)) => {
                Status::internal("Failed to compile script")
            }
            MissingChunk(_) => Status::failed_precondition("Unknown chunk index."),
            Client(_)
            | Image(_)
            | Manifest(_)
            | Metadata(_)
            | ParseManifest(_)
            | ParseManifestBody(_)
            | ParseManifestHeader(_)
            | ParseUtf8(_)
            | SerializeBody(_)
            | SerializeHeader(_) => Status::internal("Internal error."),
        }
    }
}

pub struct Storage {
    pub bucket: BucketConfig,
    pub prefix: String,
    pub expiration: Duration,
    pub client: Arc<dyn Client>,
    pub engine: Arc<Engine>,
}

impl Storage {
    pub fn new<C>(config: &Config, client: C) -> Self
    where
        C: Client + 'static,
    {
        Storage {
            bucket: config.bucket.clone(),
            prefix: config.dir_chains_prefix.clone(),
            expiration: *config.presigned_url_expiration,
            client: Arc::new(client),
            engine: Arc::new(Engine::new()),
        }
    }

    pub fn new_s3(config: &Config) -> Self {
        let credentials = Credentials::new(&*config.key_id, &*config.key, None, None, CREDENTIALS);
        let s3_config = aws_sdk_s3::Config::builder()
            .endpoint_url(config.storage_url.to_string())
            .region(Region::new(config.region.clone()))
            .credentials_provider(credentials)
            .build();

        Self::new(config, aws_sdk_s3::Client::from_conf(s3_config))
    }

    async fn read_file(&self, image: &ImageId, file: &str) -> Result<Vec<u8>, Error> {
        let key = image.key(&self.prefix, file);
        self.client
            .read_key(&self.bucket.cookbook, &key)
            .await
            .map_err(Into::into)
    }

    async fn read_string(&self, bucket: &str, key: &str) -> Result<String, Error> {
        self.client
            .read_key(bucket, key)
            .await
            .map_err(Into::into)
            .and_then(|bytes| String::from_utf8(bytes).map_err(Error::ParseUtf8))
    }

    async fn read_manifest_header(&self, key: &str) -> Result<ManifestHeader, Error> {
        self.read_string(&self.bucket.archive, key)
            .await
            .and_then(|manifest| {
                serde_json::from_str(&manifest).map_err(Error::ParseManifestHeader)
            })
    }

    async fn read_manifest_body(&self, key: &str) -> Result<ManifestBody, Error> {
        self.read_string(&self.bucket.archive, key)
            .await
            .and_then(|manifest| serde_json::from_str(&manifest).map_err(Error::ParseManifestBody))
    }

    async fn read_download_manifest(&self, key: &str) -> Result<DownloadManifest, Error> {
        self.read_string(&self.bucket.archive, key)
            .await
            .and_then(|manifest| serde_json::from_str(&manifest).map_err(Error::ParseManifest))
    }

    async fn download_url(&self, bucket: &str, key: &str) -> Result<Url, Error> {
        self.client
            .download_url(bucket, key, self.expiration)
            .await
            .map_err(Into::into)
    }

    pub async fn download_image(&self, image: &ImageId) -> Result<Url, Error> {
        let key = image.key(&self.prefix, BABEL_IMAGE_FILE);
        self.download_url(&self.bucket.cookbook, &key).await
    }

    pub async fn download_bundle(&self, version: &str) -> Result<Url, Error> {
        let key = format!("{version}/{BUNDLE_FILE}");
        self.download_url(&self.bucket.bundle, &key).await
    }

    pub async fn download_kernel(&self, version: &str) -> Result<Url, Error> {
        let key = format!("{version}/{KERNEL_FILE}");
        self.download_url(&self.bucket.kernel, &key).await
    }

    pub async fn rhai_script(&self, image: &ImageId) -> Result<Vec<u8>, Error> {
        self.read_file(image, RHAI_FILE).await
    }

    pub async fn rhai_metadata(&self, image: &ImageId) -> Result<BlockchainMetadata, Error> {
        let key = image.key(&self.prefix, RHAI_FILE);
        let script = self.read_string(&self.bucket.cookbook, &key).await?;

        BlockchainMetadata::from_script(&self.engine, &script, image).map_err(Into::into)
    }

    /// List image identifiers for some protocol and node type.
    ///
    /// A bucket listing looks like:
    /// ```text
    /// prefix/eth/validator/0.0.3/data.txt
    /// prefix/eth/validator/0.0.3/babel.rhai
    /// prefix/eth/validator/0.0.6/babel.rhai
    /// ```
    ///
    /// Since we filter by protocol and `NodeType` when listing, we only need to
    /// de-duplicate the results by version for each config identifier.
    pub async fn list(&self, protocol: &str, node_type: NodeType) -> Result<Vec<ImageId>, Error> {
        let path = format!("{prefix}/{protocol}/{node_type}", prefix = self.prefix);
        let keys = self.client.list_all(&self.bucket.cookbook, &path).await?;
        let images = keys
            .into_iter()
            .map(|key| ImageId::from_key(key).map(|image| (image.node_version.clone(), image)))
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(images.into_values().collect())
    }

    pub async fn list_bundles(&self) -> Result<Vec<api::BundleIdentifier>, Error> {
        let keys = self.client.list_all(&self.bucket.bundle, "").await?;
        let idents = keys
            .iter()
            .filter_map(api::BundleIdentifier::maybe_from_key)
            .collect();

        Ok(idents)
    }

    pub async fn list_kernels(&self) -> Result<Vec<api::KernelIdentifier>, Error> {
        let keys = self.client.list_all(&self.bucket.kernel, "").await?;
        let idents = keys
            .iter()
            .filter_map(api::KernelIdentifier::maybe_from_key)
            .collect();

        Ok(idents)
    }

    /// Find the most recent download manifest header (<= `image.node_version`).
    ///
    /// If `data_version` is None then it uses the latest data version.
    pub async fn find_download_manifest_header(
        &self,
        image: &ImageId,
        network: &str,
        data_version: Option<u64>,
    ) -> Result<(ManifestHeader, u64), Error> {
        let node_version = image.semver()?;
        let mut node_versions = self.node_versions(image).await?;

        loop {
            let Some(version) = node_versions.pop() else {
                return Err(Error::FindManifestHeader(image.clone(), network.into()));
            };

            if version <= node_version {
                match self
                    .download_manifest_header(image, &version, network, data_version)
                    .await
                {
                    Ok((header, data_version)) => return Ok((header, data_version)),
                    Err(err) => warn!("Manifest not found: {err:#}"),
                }
            }
        }
    }

    /// Parse a download manifest header for some `node_version`.
    ///
    /// If `data_version` is None then it uses the latest data version.
    pub async fn download_manifest_header(
        &self,
        image: &ImageId,
        node_version: &Version,
        network: &str,
        data_version: Option<u64>,
    ) -> Result<(ManifestHeader, u64), Error> {
        let data_version = if let Some(version) = data_version {
            version
        } else {
            self.latest_data_version(image, node_version, network, true)
                .await?
                .ok_or(Error::NoDataVersion)?
        };

        let prefix = format!(
            "{protocol}/{node_type}/{node_version}/{network}/{data_version}",
            protocol = image.protocol,
            node_type = image.node_type,
        );

        let header_key = format!("{prefix}/{MANIFEST_HEADER}");
        let header = match self.read_manifest_header(&header_key).await {
            Ok(header) => header,
            Err(Error::Client(client::Error::MissingKey(_, _))) => {
                let fallback_key = format!("{prefix}/{MANIFEST_FILE}");
                self.read_download_manifest(&fallback_key)
                    .await
                    .and_then(|ref manifest| manifest.try_into().map_err(Into::into))?
            }
            Err(err) => return Err(err),
        };

        Ok((header, data_version))
    }

    /// Find the most recent download manifest header (<= `image.node_version`).
    ///
    /// If `data_version` is None then it uses the latest data version.
    pub async fn find_download_manifest_body(
        &self,
        image: &ImageId,
        network: &str,
        data_version: Option<u64>,
    ) -> Result<(ManifestBody, u64), Error> {
        let node_version = image.semver()?;
        let mut node_versions = self.node_versions(image).await?;

        loop {
            let Some(version) = node_versions.pop() else {
                return Err(Error::FindManifestBody(image.clone(), network.into()));
            };

            if version <= node_version {
                match self
                    .download_manifest_body(image, &version, network, data_version)
                    .await
                {
                    Ok((body, data_version)) => return Ok((body, data_version)),
                    Err(err) => warn!("Manifest not found: {err:#}"),
                }
            }
        }
    }

    /// Parse a download manifest body for some `node_version`.
    ///
    /// If `data_version` is None then it uses the latest data version.
    async fn download_manifest_body(
        &self,
        image: &ImageId,
        node_version: &Version,
        network: &str,
        data_version: Option<u64>,
    ) -> Result<(ManifestBody, u64), Error> {
        let data_version = if let Some(version) = data_version {
            version
        } else {
            self.latest_data_version(image, node_version, network, true)
                .await?
                .ok_or(Error::NoDataVersion)?
        };

        let prefix = format!(
            "{protocol}/{node_type}/{node_version}/{network}/{data_version}",
            protocol = image.protocol,
            node_type = image.node_type,
        );

        let body_key = format!("{prefix}/{MANIFEST_BODY}");
        let body = match self.read_manifest_body(&body_key).await {
            Ok(header) => header,
            Err(Error::Client(client::Error::MissingKey(_, _))) => {
                let fallback_key = format!("{prefix}/{MANIFEST_FILE}");
                self.read_download_manifest(&fallback_key)
                    .await
                    .map(Into::into)?
            }
            Err(err) => return Err(err),
        };

        Ok((body, data_version))
    }

    /// Regenerate the download URLs for the requested `DownloadManifest` chunks.
    pub async fn refresh_download_manifest(
        &self,
        image: &ImageId,
        network: &str,
        data_version: u64,
        chunk_indexes: &[usize],
    ) -> Result<Vec<ArchiveChunk>, Error> {
        let (manifest, _) = self
            .find_download_manifest_body(image, network, Some(data_version))
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

    /// Returns an ordered list of node versions for an image.
    async fn node_versions(&self, image: &ImageId) -> Result<Vec<Version>, Error> {
        let path = format!("{}/{}/", image.protocol, image.node_type);
        let keys = self.client.list(&self.bucket.archive, &path).await?;

        let mut versions = keys
            .iter()
            .filter_map(|key| {
                key.trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .and_then(|node_version| Version::parse(node_version).ok())
            })
            .collect::<Vec<_>>();

        versions.sort();
        versions.dedup();
        Ok(versions)
    }

    /// Returns an ordered list of data versions for an image.
    async fn data_versions(
        &self,
        image: &ImageId,
        node_version: &Version,
        network: &str,
    ) -> Result<Vec<u64>, Error> {
        let path = format!(
            "{protocol}/{node_type}/{node_version}/{network}/",
            protocol = image.protocol,
            node_type = image.node_type,
        );

        let keys = self.client.list(&self.bucket.archive, &path).await?;
        let mut versions: Vec<_> = keys
            .into_iter()
            .filter_map(|key| {
                key.trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .and_then(|data_version| data_version.parse::<u64>().ok())
            })
            .collect();

        versions.sort_unstable();
        versions.dedup();
        Ok(versions)
    }

    /// Returns the latest data version for an image.
    async fn latest_data_version(
        &self,
        image: &ImageId,
        node_version: &Version,
        network: &str,
        has_manifest: bool,
    ) -> Result<Option<u64>, Error> {
        let mut versions = self.data_versions(image, node_version, network).await?;

        loop {
            let data_version = match versions.pop() {
                Some(version) if has_manifest => version,
                Some(version) => return Ok(Some(version)),
                None => return Ok(None),
            };

            let prefix = format!(
                "{protocol}/{node_type}/{node_version}/{network}/{data_version}",
                protocol = image.protocol,
                node_type = image.node_type,
            );

            let header = format!("{prefix}/{MANIFEST_HEADER}");
            match self.client.read_key(&self.bucket.archive, &header).await {
                Ok(_bytes) => return Ok(Some(data_version)),
                Err(client::Error::MissingKey(_, _)) => {
                    let fallback = format!("{prefix}/{MANIFEST_FILE}");
                    match self.client.read_key(&self.bucket.archive, &fallback).await {
                        Ok(_bytes) => return Ok(Some(data_version)),
                        Err(client::Error::MissingKey(_, _)) => (),
                        Err(err) => warn!("Failed to read key `{fallback}`: {err}"),
                    }
                }
                Err(err) => warn!("Failed to read key `{header}`: {err}"),
            }
        }
    }

    pub async fn save_download_manifest(
        &self,
        image: &ImageId,
        network: &str,
        data_version: u64,
        manifest: DownloadManifest,
    ) -> Result<(), Error> {
        let node_version = image.semver()?;
        let prefix = format!(
            "{protocol}/{node_type}/{node_version}/{network}/{data_version}",
            protocol = image.protocol,
            node_type = image.node_type,
        );

        let header: ManifestHeader = (&manifest).try_into()?;
        let body: ManifestBody = manifest.into();

        let body_key = format!("{prefix}/{MANIFEST_BODY}");
        let body_data = serde_json::to_vec(&body).map_err(Error::SerializeBody)?;
        self.client
            .write_key(&self.bucket.archive, &body_key, body_data)
            .await?;

        let header_key = format!("{prefix}/{MANIFEST_HEADER}");
        let header_data = serde_json::to_vec(&header).map_err(Error::SerializeHeader)?;
        self.client
            .write_key(&self.bucket.archive, &header_key, header_data)
            .await
            .map_err(Into::into)
    }

    pub async fn upload_slots(
        &self,
        image: &ImageId,
        network: &str,
        data_version: Option<u64>,
        slot_indexes: &[usize],
        expires: Duration,
    ) -> Result<(Vec<UploadSlot>, u64), Error> {
        let node_version = image.semver()?;
        let data_version = if let Some(version) = data_version {
            version
        } else {
            self.latest_data_version(image, &node_version, network, false)
                .await?
                .unwrap_or_default()
                + 1
        };

        let path = format!(
            "{protocol}/{node_type}/{node_version}/{network}/{data_version}",
            protocol = image.protocol,
            node_type = image.node_type.to_string().to_lowercase(),
        );

        let mut slots = Vec::with_capacity(slot_indexes.len());
        for &index in slot_indexes {
            let key = format!("{path}/data.part_{index}");
            let url = self
                .client
                .upload_url(&self.bucket.archive, &key, expires)
                .await?;
            slots.push(UploadSlot { index, key, url });
        }

        Ok((slots, data_version))
    }
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use mockito::{Matcher, Server, ServerGuard};

    use super::metadata::tests::TEST_SCRIPT;
    use super::*;

    pub struct TestStorage {
        mock: ServerGuard,
    }

    impl TestStorage {
        pub async fn new() -> Self {
            TestStorage {
                mock: mock_server().await,
            }
        }

        pub fn new_mock(&self) -> Storage {
            let config = self.mock_config();
            let client = MockClient {};
            Storage::new(&config, client)
        }

        fn mock_config(&self) -> Config {
            Config {
                bucket: BucketConfig {
                    cookbook: "cookbook".to_string(),
                    bundle: "bundle".to_string(),
                    kernel: "kernel".to_string(),
                    archive: "archive".to_string(),
                },
                storage_url: self.mock.url().parse().unwrap(),
                key_id: "key_id".parse().unwrap(),
                key: "key".parse().unwrap(),
                region: "eu-west-3".to_string(),
                dir_chains_prefix: "prefix".to_string(),
                presigned_url_expiration: "1d".parse().unwrap(),
            }
        }
    }

    struct MockClient {}

    #[tonic::async_trait]
    impl Client for MockClient {
        async fn list(&self, _: &str, _: &str) -> Result<Vec<String>, super::client::Error> {
            unimplemented!()
        }

        async fn list_all(&self, _: &str, _: &str) -> Result<Vec<String>, super::client::Error> {
            unimplemented!()
        }

        async fn read_key(&self, _: &str, _: &str) -> Result<Vec<u8>, super::client::Error> {
            Ok(TEST_SCRIPT.bytes().collect())
        }

        async fn write_key(
            &self,
            _: &str,
            _: &str,
            _: Vec<u8>,
        ) -> Result<(), super::client::Error> {
            unimplemented!()
        }

        async fn download_url(
            &self,
            _: &str,
            _: &str,
            _: Duration,
        ) -> Result<Url, super::client::Error> {
            unimplemented!()
        }

        async fn upload_url(
            &self,
            _: &str,
            _: &str,
            _: Duration,
        ) -> Result<Url, super::client::Error> {
            unimplemented!()
        }
    }

    async fn mock_server() -> ServerGuard {
        let mut server = Server::new_async().await;
        server
            .mock("POST", Matcher::Regex(r"^/*".to_string()))
            .with_status(200)
            .with_body("{\"data\":\"id\"}")
            .create_async()
            .await;
        server
    }
}
