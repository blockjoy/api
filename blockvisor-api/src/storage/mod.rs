pub mod client;
pub mod image;
pub mod manifest;
pub mod metadata;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use displaydoc::Display;
use rhai::Engine;
use semver::Version;
use thiserror::Error;
use tonic::Status;
use tracing::warn;
use url::Url;

use crate::config::storage::{BucketConfig, Config};
use crate::grpc::api;
use crate::models::node::NodeType;

use self::client::Client;
use self::image::ImageId;
use self::manifest::{DownloadManifest, UploadManifest, UploadSlot};
use self::metadata::BlockchainMetadata;

pub const CREDENTIALS: &str = "blockvisor-api credentials provider";
pub const BABEL_IMAGE_FILE: &str = "blockjoy.gz";
pub const BUNDLE_FILE: &str = "bvd-bundle.tgz";
pub const KERNEL_FILE: &str = "kernel.gz";
pub const MANIFEST_FILE: &str = "manifest.json";
pub const RHAI_FILE: &str = "babel.rhai";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Storage client error: {0}
    Client(#[from] client::Error),
    /// No download manifest found for `{0:?}` in network {1}.
    DownloadManifest(ImageId, String),
    /// No manifest found for image `{0:?}`, version: `{1:?}`, network `{2}`.
    FindManifest(ImageId, Option<Version>, String),
    /// Storage image error: {0}
    Image(#[from] image::Error),
    /// Storage metadata error: {0}
    Metadata(#[from] metadata::Error),
    /// Failed to parse manifest: {0}
    ParseManifest(serde_json::Error),
    /// Failed to parse storage bytes as UTF8: {0}
    ParseUtf8(std::string::FromUtf8Error),
    /// Failed to serialize DownloadManifest: {0}
    SerializeManifest(serde_json::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Client(crate::storage::client::Error::ReadKey(
                bucket,
                file,
                SdkError::ServiceError(err),
            )) if matches!(err.err(), GetObjectError::NoSuchKey(_)) => {
                Status::not_found(format!("No rhai script at `{bucket}:{file}`"))
            }
            DownloadManifest(_, _) | FindManifest(_, _, _) => Status::not_found("Not found."),
            Metadata(crate::storage::metadata::Error::CompileScript(_, _)) => {
                Status::internal("Failed to compile script")
            }
            Client(_) | Image(_) | Metadata(_) | ParseManifest(_) | ParseUtf8(_)
            | SerializeManifest(_) => Status::internal("Internal error."),
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

    async fn read_manifest(&self, key: &str) -> Result<DownloadManifest, Error> {
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

    /// Find the most recent download manifest (<= `image.node_version`)
    /// and generates the download URLs which may have expired.
    pub async fn generate_download_manifest(
        &self,
        image: &ImageId,
        network: &str,
    ) -> Result<DownloadManifest, Error> {
        let mut manifest = self
            .find_download_manifest_blueprint(image, network)
            .await?;
        let download_gb = manifest.chunks.iter().map(|c| c.size).sum::<u64>() / 1_000_000_000;
        let expires = Duration::from_secs(self.expiration.as_secs().max(30 * download_gb));
        for chunk in &mut manifest.chunks {
            let url = self
                .client
                .download_url(&self.bucket.archive, &chunk.key, expires)
                .await?;
            chunk.url = Some(url);
        }

        Ok(manifest)
    }

    /// Find the most recent download manifest (<= `image.node_version`) blueprint.
    pub async fn find_download_manifest_blueprint(
        &self,
        image: &ImageId,
        network: &str,
    ) -> Result<DownloadManifest, Error> {
        let node_version = image.semver()?;
        let node_versions = self.node_versions(image).await?;
        let mut versions = node_versions.iter().rev();

        let manifest = loop {
            let Some(version) = versions.next() else {
                return Err(Error::DownloadManifest(image.clone(), network.into()));
            };

            if *version <= node_version {
                match self.find_manifest(image, Some(version), network).await {
                    Ok(manifest) => break manifest,
                    Err(err) => warn!("Manifest not found: {err:#}"),
                }
            }
        };
        Ok(manifest)
    }

    /// Find all node versions for some image `protocol` and `node_type`.
    async fn node_versions(&self, image: &ImageId) -> Result<Vec<Version>, Error> {
        let path = format!("{}/{}/", image.protocol, image.node_type);
        let keys = self.client.list(&self.bucket.archive, &path).await?;

        let mut versions = keys
            .iter()
            .filter_map(|key| last_segment(key).and_then(|segment| Version::parse(segment).ok()))
            .collect::<Vec<_>>();

        versions.sort();
        Ok(versions)
    }

    /// Find the most recent download manifest for some image and network type.
    ///
    /// If `version` is Some then it is used instead of `image.node_version`.
    async fn find_manifest(
        &self,
        image: &ImageId,
        version: Option<&Version>,
        network: &str,
    ) -> Result<DownloadManifest, Error> {
        let data_versions = self.data_versions(image, version, network).await?;
        for data_version in data_versions.iter().rev() {
            let key = format!(
                "{protocol}/{node_type}/{version}/{network}/{data_version}/{MANIFEST_FILE}",
                protocol = image.protocol,
                node_type = image.node_type,
                version = version
                    .map(ToString::to_string)
                    .as_deref()
                    .unwrap_or(image.node_version.as_str())
            );

            match self.read_manifest(&key).await {
                Ok(manifest) => return Ok(manifest),
                Err(err) => warn!("Invalid manifest at `{key}`: {err:#}"),
            }
        }

        Err(Error::FindManifest(
            image.clone(),
            version.cloned(),
            network.into(),
        ))
    }

    /// Return an ordered list of data versions for an image.
    ///
    /// If `version` is Some then it is used instead of `image.node_version`.
    async fn data_versions(
        &self,
        image: &ImageId,
        version: Option<&Version>,
        network: &str,
    ) -> Result<Vec<u64>, Error> {
        let path = format!(
            "{protocol}/{node_type}/{version}/{network}/",
            protocol = image.protocol,
            node_type = image.node_type,
            version = version
                .map(ToString::to_string)
                .as_deref()
                .unwrap_or(image.node_version.as_str())
        );

        let data_versions = self.client.list(&self.bucket.archive, &path).await?;
        let mut versions: Vec<_> = data_versions
            .into_iter()
            .filter_map(|ver| last_segment(&ver).and_then(|segment| segment.parse::<u64>().ok()))
            .collect();

        versions.sort_unstable();
        Ok(versions)
    }

    pub async fn save_download_manifest(
        &self,
        image: &ImageId,
        network: &str,
        manifest: &DownloadManifest,
    ) -> Result<(), Error> {
        let versions = self.data_versions(image, None, network).await?;
        let data_version = versions.last().unwrap_or(&0);

        let key = format!(
            "{protocol}/{node_type}/{node_version}/{network}/{data_version}/{MANIFEST_FILE}",
            protocol = image.protocol,
            node_type = image.node_type,
            node_version = image.node_version,
        );
        let data = serde_json::to_vec(manifest).map_err(Error::SerializeManifest)?;

        self.client
            .write_key(&self.bucket.archive, &key, data)
            .await
            .map_err(Into::into)
    }

    pub async fn upload_manifest(
        &self,
        image: &ImageId,
        network: &str,
        data_version: Option<u64>,
        upload_slots: u32,
        expires: Duration,
    ) -> Result<UploadManifest, Error> {
        let data_version = if let Some(version) = data_version {
            version
        } else {
            let versions = self.data_versions(image, None, network).await?;
            versions.last().unwrap_or(&0) + 1
        };

        let path = format!(
            "{protocol}/{node_type}/{min_node_version}/{network}/{data_version}",
            protocol = image.protocol,
            node_type = image.node_type.to_string().to_lowercase(),
            min_node_version = image.node_version,
        );

        let mut slots = Vec::with_capacity(upload_slots as usize);
        for index in 0..upload_slots {
            let key = format!("{path}/data.part_{index}");
            let url = self
                .client
                .upload_url(&self.bucket.archive, &key, expires)
                .await?;
            slots.push(UploadSlot { key, url });
        }

        Ok(UploadManifest { slots })
    }
}

fn last_segment(key: &str) -> Option<&str> {
    key.trim_end_matches('/').rsplit('/').next()
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
