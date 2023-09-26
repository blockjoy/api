pub mod manifest;
pub mod script;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Error;
use aws_sdk_s3::presigning::{PresigningConfig, PresigningConfigError};
use aws_sdk_s3::primitives::ByteStreamError;
use displaydoc::Display;
use semver::Version;
use thiserror::Error;
use tracing::debug;

use crate::config;
use crate::grpc::api;
use crate::models::node::NodeType;

use self::manifest::DownloadManifest;
use self::script::BlockchainMetadata;

pub const RHAI_FILE_NAME: &str = "babel.rhai";
pub const BABEL_IMAGE_NAME: &str = "blockjoy.gz";
pub const KERNEL_NAME: &str = "kernel.gz";
pub const BUNDLE_NAME: &str = "bvd-bundle.tgz";

#[tonic::async_trait]
pub trait Client: Send + Sync {
    async fn read_file(&self, bucket: &str, path: &str) -> Result<Vec<u8>, Error>;

    async fn read_string(&self, bucket: &str, path: &str) -> Result<String, Error> {
        self.read_file(bucket, path)
            .await
            .and_then(|bytes| String::from_utf8(bytes).map_err(Error::ParseUtf8))
    }

    async fn download_url(
        &self,
        bucket: &str,
        path: &str,
        expiration: Duration,
    ) -> Result<String, Error>;

    /// List entries in given `path` non-recursively.
    async fn list(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error>;

    /// List all entries in given `path` recursively.
    async fn list_all(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error>;
}

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to compile script: {0}
    CompileScript(rhai::ParseError),
    /// Invalid rhai script: {0}
    InvalidScript(Box<rhai::EvalAltResult>),
    /// Failed to list path `{0}`: {1}
    ListPath(String, SdkError<ListObjectsV2Error>),
    /// Manifest error: {0}
    Manifest(manifest::Error),
    /// No manifest found for node `{0:?}-{1}` in `{2}`.
    NoManifest(api::ConfigIdentifier, String, String),
    /// No manifest found in path `{0}`.
    NoManifestInPath(String),
    /// No metadata in rhai script.
    NoMetadata,
    /// Failed to parse manifest: {0}
    ParseManifest(serde_json::Error),
    /// Failed to parse NodeType: {0}
    ParseNodeType(crate::models::node::node_type::Error),
    /// Failed to parse bytes as UTF8: {0}
    ParseUtf8(std::string::FromUtf8Error),
    /// Failed to parse version: {0}
    ParseVersion(semver::Error),
    /// Failed to create presigned config: {0}
    PresignedConfig(PresigningConfigError),
    /// Failed to create presigned URL for path `{0}`: {1}
    PresignedUrl(String, SdkError<GetObjectError>),
    /// Failed to query file `{0}:{1}`: {2}
    QueryFile(String, String, ByteStreamError),
    /// Failed to read file `{0}:{1}`: {2}
    ReadFile(String, String, SdkError<GetObjectError>),
    /// Key `{0}` is not splittable into at least 4 `/` separated parts.
    SplitKey(String),
    /// File name should end in `/{BUNDLE_NAME:?}` but is `{0}`.
    SuffixBundle(String),
    /// File name should end in `/{KERNEL_NAME:?}` but is `{0}`.
    SuffixKernel(String),
    #[cfg(any(test, feature = "integration-test"))]
    /// Unexpected error: {0}
    Unexpected(&'static str),
}

#[tonic::async_trait]
impl Client for aws_sdk_s3::Client {
    async fn read_file(&self, bucket: &str, path: &str) -> Result<Vec<u8>, Error> {
        let path = path.to_lowercase();
        let response = self
            .get_object()
            .bucket(bucket)
            .key(&path)
            .send()
            .await
            .map_err(|err| Error::ReadFile(bucket.into(), path.clone(), err))?;

        response
            .body
            .collect()
            .await
            .map(|bytes| bytes.into_bytes().to_vec())
            .map_err(|err| Error::QueryFile(bucket.into(), path, err))
    }

    async fn download_url(
        &self,
        bucket: &str,
        path: &str,
        expires: Duration,
    ) -> Result<String, Error> {
        let path = path.to_lowercase();
        let presigned = PresigningConfig::expires_in(expires).map_err(Error::PresignedConfig)?;

        self.get_object()
            .bucket(bucket)
            .key(&path)
            .presigned(presigned)
            .await
            .map(|url| url.uri().to_string())
            .map_err(|err| Error::PresignedUrl(path, err))
    }

    async fn list(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error> {
        let path = path.to_lowercase();
        let resp = self
            .list_objects_v2()
            .bucket(bucket)
            .prefix(&path)
            .delimiter('/')
            .send()
            .await
            .map_err(|err| Error::ListPath(path, err))?;
        let files = resp
            .common_prefixes()
            .unwrap_or_default()
            .iter()
            .filter_map(|object| object.prefix().map(|prefix| prefix.to_owned()))
            .collect();
        Ok(files)
    }

    async fn list_all(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error> {
        let path = path.to_lowercase();
        let resp = self
            .list_objects_v2()
            .bucket(bucket)
            .prefix(&path)
            .send()
            .await
            .map_err(|err| Error::ListPath(path, err))?;

        let files = resp
            .contents()
            .unwrap_or_default()
            .iter()
            .filter_map(|object| object.key().map(|key| key.to_owned()))
            .collect();

        Ok(files)
    }
}

#[derive(Clone)]
pub struct Cookbook {
    pub data_prefix: String,
    pub prefix: String,
    pub bucket: String,
    pub bundle_bucket: String,
    pub kernel_bucket: String,
    pub expiration: Duration,
    pub client: Arc<dyn Client>,
    pub engine: Arc<rhai::Engine>,
}

impl Cookbook {
    /// Creates a new instance of `Cookbook` using s3 as the underlying storage.
    pub fn new_s3(config: &config::cookbook::Config) -> Self {
        let s3_config = aws_sdk_s3::Config::builder()
            .endpoint_url(config.r2_url.to_string())
            .region(aws_sdk_s3::config::Region::new(config.region.clone()))
            .credentials_provider(Credentials::new(
                config.key_id.as_str(),
                config.key.as_str(),
                None,
                None,
                "Custom Provided Credentials",
            ))
            .build();
        let client = aws_sdk_s3::Client::from_conf(s3_config);
        Self::new_with_client(config, client)
    }

    /// Creates a new instance of `Cookbook` using the `client` parameter as the
    /// storage implementation.
    pub fn new_with_client(
        config: &config::cookbook::Config,
        client: impl Client + 'static,
    ) -> Self {
        let engine = Arc::new(rhai::Engine::new());

        Self {
            data_prefix: config.dir_chains_data_prefix.clone(),
            prefix: config.dir_chains_prefix.clone(),
            bucket: config.r2_bucket.clone(),
            bundle_bucket: config.bundle_bucket.clone(),
            kernel_bucket: config.kernel_bucket.clone(),
            expiration: *config.presigned_url_expiration,
            client: Arc::new(client),
            engine,
        }
    }

    pub async fn read_file(
        &self,
        protocol: &str,
        node_type: NodeType,
        node_version: &str,
        file: &str,
    ) -> Result<Vec<u8>, Error> {
        let path = format!(
            "{prefix}/{protocol}/{node_type}/{node_version}/{file}",
            prefix = self.prefix,
        );
        self.client.read_file(&self.bucket, &path).await
    }

    pub async fn download_url(
        &self,
        protocol: &str,
        node_type: NodeType,
        node_version: &str,
        file: &str,
    ) -> Result<String, Error> {
        let path = format!(
            "{prefix}/{protocol}/{node_type}/{node_version}/{file}",
            prefix = self.prefix,
        );
        self.client
            .download_url(&self.bucket, &path, self.expiration)
            .await
    }

    pub async fn download_url_kernel(&self, version: &str) -> Result<String, Error> {
        let path = format!("{version}/{KERNEL_NAME}");
        self.client
            .download_url(&self.kernel_bucket, &path, self.expiration)
            .await
    }

    pub async fn bundle_download_url(&self, version: &str) -> Result<String, Error> {
        let path = format!("{version}/{BUNDLE_NAME}");
        self.client
            .download_url(&self.bundle_bucket, &path, self.expiration)
            .await
    }

    pub async fn list(
        &self,
        protocol: &str,
        node_type: NodeType,
    ) -> Result<Vec<api::ConfigIdentifier>, Error> {
        // We retrieve the config identifiers from the folder structure on S3. Suppose there exist
        // some files:
        // prefix/eth/validator/0.0.3/data.txt
        // prefix/eth/validator/0.0.3/babel.rhai
        // prefix/eth/validator/0.0.6/babel.rhai
        // Then we want to return the configidentifiers from this that have version 0.0.3 and 0.0.6.
        // Since we are filtering by protocol and node_type, we will only need to deduplicate using
        // the version field, so we throw everything into a map from version to the config
        // identifier, and use that map to construct our final result.
        let path = format!("{prefix}/{protocol}/{node_type}", prefix = self.prefix);
        let mut idents = HashMap::new();
        for ident in self.client.list_all(&self.bucket, &path).await?.iter() {
            let ident = api::ConfigIdentifier::from_key(ident)?;
            idents.insert(ident.node_type(), ident);
        }
        Ok(idents.into_values().collect())
    }

    pub async fn list_bundles(&self) -> Result<Vec<api::BundleIdentifier>, Error> {
        Ok(self
            .client
            .list_all(&self.bundle_bucket, "")
            .await?
            .iter()
            .flat_map(api::BundleIdentifier::from_key)
            .collect())
    }

    pub async fn list_kernels(&self) -> Result<Vec<api::KernelIdentifier>, Error> {
        Ok(self
            .client
            .list_all(&self.kernel_bucket, "")
            .await?
            .iter()
            .flat_map(api::KernelIdentifier::from_key)
            .collect())
    }

    pub async fn rhai_metadata(
        &self,
        protocol: &str,
        node_type: NodeType,
        node_version: &str,
    ) -> Result<BlockchainMetadata, Error> {
        let path = format!(
            "{prefix}/{protocol}/{node_type}/{node_version}/{RHAI_FILE_NAME}",
            prefix = self.prefix,
        );
        let script = self.client.read_string(&self.bucket, &path).await?;
        Self::script_to_metadata(&self.engine, &script)
    }

    pub async fn get_download_manifest(
        &self,
        id: api::ConfigIdentifier,
        network: String,
    ) -> Result<api::DownloadManifest, Error> {
        let node_version = Version::parse(&id.node_version).map_err(Error::ParseVersion)?;
        let path = format!(
            "{}/{}/{}",
            self.data_prefix,
            id.protocol,
            id.node_type().into_model()
        );
        let min_versions = self.get_min_node_versions(&path).await?;
        let mut version_iter = min_versions.iter().rev();
        let mut manifest = loop {
            let Some((version_str, version)) = version_iter.next() else {
                return Err(Error::NoManifest(id, network, path));
            };
            if node_version >= *version {
                match self
                    .find_valid_manifest(&format!("{path}/{version_str}/{network}"))
                    .await
                {
                    Ok(manifest) => break manifest,
                    Err(err) => {
                        debug!("Manifest not found in {path}/{version_str}/{network}: {err:#}")
                    }
                }
            }
        };

        for chunk in manifest.chunks.iter_mut() {
            chunk.url = self
                .client
                .download_url(&self.bucket, &chunk.key, self.expiration)
                .await?;
        }

        manifest.try_into().map_err(Error::Manifest)
    }

    async fn get_min_node_versions(&self, path: &str) -> Result<Vec<(String, Version)>, Error> {
        let min_versions = self.client.list(&self.bucket, path).await?;
        let mut min_versions: Vec<_> = min_versions
            .into_iter()
            .filter_map(|version_str| {
                subdir_name(&version_str).and_then(|version_str| {
                    Version::parse(version_str)
                        .ok()
                        .map(|version| (version_str.to_owned(), version))
                })
            })
            .collect();
        min_versions.sort_by(|(_, a), (_, b)| a.cmp(b));
        Ok(min_versions)
    }

    async fn find_valid_manifest(&self, path: &str) -> Result<DownloadManifest, Error> {
        let data_versions = self.get_data_versions(path).await?;
        let mut version_iter = data_versions.iter().rev();

        loop {
            let Some((version_str, _)) = version_iter.next() else {
                return Err(Error::NoManifestInPath(path.into()));
            };
            match self
                .client
                .read_string(&self.bucket, &format!("{path}/{version_str}/manifest.json"))
                .await
                .and_then(|manifest| serde_json::from_str(&manifest).map_err(Error::ParseManifest))
            {
                Ok(manifest) => return Ok(manifest),
                Err(err) => debug!("Invalid manifest {path}/{version_str}/manifest.json: {err:#}"),
            }
        }
    }

    async fn get_data_versions(&self, path: &str) -> Result<Vec<(String, u64)>, Error> {
        let min_versions = self.client.list(&self.bucket, path).await?;
        let mut min_versions: Vec<_> = min_versions
            .into_iter()
            .filter_map(|version_str| {
                subdir_name(&version_str).and_then(|version_str| {
                    version_str
                        .parse::<u64>()
                        .ok()
                        .map(|version| (version_str.to_owned(), version))
                })
            })
            .collect();
        min_versions.sort_by(|(_, a), (_, b)| a.cmp(b));
        Ok(min_versions)
    }

    fn script_to_metadata(
        engine: &rhai::Engine,
        script: &str,
    ) -> Result<BlockchainMetadata, Error> {
        let (_, _, dynamic) = engine
            .compile(script)
            .map_err(Error::CompileScript)?
            .iter_literal_variables(true, false)
            .find(|&(name, _, _)| name == "METADATA")
            .ok_or(Error::NoMetadata)?;

        rhai::serde::from_dynamic(&dynamic).map_err(Error::InvalidScript)
    }
}

fn subdir_name(path: &str) -> Option<&str> {
    path.trim_end_matches('/').rsplit('/').next()
}

impl api::ConfigIdentifier {
    fn from_key(key: impl AsRef<str>) -> Result<Self, Error> {
        // We want to parse a `ConfigIdentifier` from a file path. This file path looks like this:
        // `/prefix/ethereum/validator/0.0.3/babel.rhai`. This means that we need to extract the
        // relevant parts by `/`-splitting the path.
        let key = key.as_ref();
        let parts: Vec<&str> = key.split('/').collect();
        let [_, protocol, node_type, node_version, ..] = &parts[..] else {
            return Err(Error::SplitKey(key.into()));
        };
        let node_type: NodeType = node_type.parse().map_err(Error::ParseNodeType)?;

        Ok(api::ConfigIdentifier {
            protocol: protocol.to_string(),
            node_type: api::NodeType::from_model(node_type).into(),
            node_version: node_version.to_string(),
        })
    }
}

impl api::BundleIdentifier {
    fn from_key(key: impl AsRef<str>) -> Result<Self, Error> {
        // "0.1.0/bvd-bundle.tgz"
        let key = key.as_ref();
        // "0.1.0"
        let version = key
            .strip_suffix(&format!("/{BUNDLE_NAME}"))
            .ok_or_else(|| Error::SuffixBundle(key.into()))?;

        Version::parse(version).map_err(Error::ParseVersion)?;

        Ok(api::BundleIdentifier {
            version: version.to_owned(),
        })
    }
}

impl api::KernelIdentifier {
    fn from_key(key: impl AsRef<str>) -> Result<Self, Error> {
        // "5.10.174-build.1+fc.ufw/kernel.gz"
        let key = key.as_ref();
        // "5.10.174-build.1+fc.ufw"
        let version = key
            .strip_suffix(&format!("/{KERNEL_NAME}"))
            .ok_or_else(|| Error::SuffixKernel(key.into()))?;

        Ok(api::KernelIdentifier {
            version: version.to_owned(),
        })
    }
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use mockall::predicate::*;
    use mockito::ServerGuard;

    use super::script::tests::TEST_SCRIPT;
    use super::*;

    mockall::mock! {
        pub Client {}

        #[tonic::async_trait]
        impl Client for Client {
            async fn read_file(&self, bucket: &str, path: &str) -> Result<Vec<u8>, Error>;
            async fn download_url(&self, bucket: &str, path: &str, expiration: Duration) -> Result<String, Error>;
            async fn list(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error>;
            async fn list_all(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error>;
        }
    }

    struct MockStorage {}

    #[tonic::async_trait]
    impl Client for MockStorage {
        async fn read_file(&self, _: &str, _: &str) -> Result<Vec<u8>, Error> {
            Ok(TEST_SCRIPT.bytes().collect())
        }

        async fn download_url(&self, _: &str, _: &str, _: Duration) -> Result<String, Error> {
            panic!("We're not using this in tests.")
        }

        async fn list(&self, _: &str, _: &str) -> Result<Vec<String>, Error> {
            panic!("We're not using this in tests.")
        }

        async fn list_all(&self, _: &str, _: &str) -> Result<Vec<String>, Error> {
            panic!("We're not using this in tests.")
        }
    }

    pub struct TestCookbook {
        mock: ServerGuard,
    }

    impl TestCookbook {
        pub async fn new() -> Self {
            let mock = Self::mock_cookbook_api().await;
            Self { mock }
        }

        pub fn get_cookbook_api(&self) -> Cookbook {
            Cookbook::new_with_client(&self.mock_config(), MockStorage {})
        }

        async fn mock_cookbook_api() -> ServerGuard {
            let mut r2_server = mockito::Server::new_async().await;
            r2_server
                .mock("POST", mockito::Matcher::Regex(r"^/*".to_string()))
                .with_status(200)
                .with_body("{\"data\":\"id\"}")
                .create_async()
                .await;
            r2_server
        }

        pub fn mock_config(&self) -> crate::config::cookbook::Config {
            crate::config::cookbook::Config {
                dir_chains_data_prefix: "fake".to_string(),
                dir_chains_prefix: "fake".to_string(),
                r2_bucket: "news".to_string(),
                r2_url: self.mock.url().parse().unwrap(),
                presigned_url_expiration: "1d".parse().unwrap(),
                region: "eu-west-3".to_string(),
                key_id: "not actually a".parse().unwrap(),
                key: "key".parse().unwrap(),
                bundle_bucket: "bundles".to_string(),
                kernel_bucket: "oui oui ceci sont les kernles".to_string(),
            }
        }
    }

    #[allow(dead_code)]
    fn dummy_config() -> crate::config::cookbook::Config {
        crate::config::cookbook::Config {
            dir_chains_data_prefix: "data".to_string(),
            dir_chains_prefix: "chains".to_string(),
            r2_bucket: "bucket".to_string(),
            r2_url: "https://dummy.url".parse().unwrap(),
            presigned_url_expiration: "1d".parse().unwrap(),
            region: "eu-west-3".to_string(),
            key_id: Default::default(),
            key: Default::default(),
            bundle_bucket: "bundles".to_string(),
            kernel_bucket: "kernles".to_string(),
        }
    }

    #[test]
    fn test_config_identifier_from_key() {
        api::ConfigIdentifier::from_key("chains/testing/validator/0.0.1").unwrap();
        api::ConfigIdentifier::from_key("chains/testing/validator/0.0.1/babel.rhai").unwrap();
    }

    #[test]
    fn test_list_bundles() {
        let elems = [
            "/bvd-bundle.tgz",
            "0.0.0/tester.txt",
            "0.1.0/bvd-bundle.tgz",
            "0.10.0/bvd-bundle.tgz",
            "0.7.0/bvd-bundle.tgz",
            "0.9.0/bvd-bundle.tgz",
        ];
        let parsed: Vec<_> = elems
            .iter()
            .flat_map(api::BundleIdentifier::from_key)
            .collect();
        assert_eq!(parsed[0].version, "0.1.0");
        assert_eq!(parsed[1].version, "0.10.0");
        assert_eq!(parsed[2].version, "0.7.0");
        assert_eq!(parsed[3].version, "0.9.0");
    }

    #[tokio::test]
    async fn test_get_download_manifest_invalid_node_version() {
        let cookbook = Cookbook::new_with_client(&dummy_config(), MockClient::new());
        let id = api::ConfigIdentifier {
            protocol: "test_blockchain".to_string(),
            node_type: NodeType::Node.into(),
            node_version: "not semver".to_string(),
        };

        assert!(cookbook
            .get_download_manifest(id, "test".to_owned())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_get_download_manifest_client_error() {
        let mut client = MockClient::new();
        client
            .expect_list()
            .with(eq("bucket"), eq("data/test_blockchain/Node"))
            .once()
            .returning(|_, _| Err(Error::Unexpected("some client error")));

        let cookbook = Cookbook::new_with_client(&dummy_config(), client);
        let id = api::ConfigIdentifier {
            protocol: "test_blockchain".to_string(),
            node_type: NodeType::Node.into(),
            node_version: "1.2.3".to_string(),
        };
        assert_eq!(
            "Unexpected error: some client error",
            cookbook
                .get_download_manifest(id, "test".to_owned())
                .await
                .unwrap_err()
                .to_string()
        );
    }

    #[tokio::test]
    async fn test_get_download_manifest_no_min_versions() {
        let mut client = MockClient::new();
        client
            .expect_list()
            .with(eq("bucket"), eq("data/test_blockchain/Node"))
            .once()
            .returning(|_, _| Ok(vec![]));
        client
            .expect_list()
            .with(eq("bucket"), eq("data/test_blockchain/Node"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "data/test_blockchain/node/invalid/".to_owned(),
                    "data/test_blockchain/node/7.7.7/".to_owned(),
                    "data/test_blockchain/node/8.8.8/".to_owned(),
                ])
            });

        let cookbook = Cookbook::new_with_client(&dummy_config(), client);
        let id = api::ConfigIdentifier {
            protocol: "test_blockchain".to_string(),
            node_type: NodeType::Node.into(),
            node_version: "1.2.3".to_string(),
        };
        assert_eq!(
            r#"No manifest found for node `ConfigIdentifier { protocol: "test_blockchain", node_type: Node, node_version: "1.2.3" }-test` in `data/test_blockchain/Node`."#,
            cookbook
                .get_download_manifest(id.clone(), "test".to_owned())
                .await
                .unwrap_err()
                .to_string()
        );
        assert_eq!(
            r#"No manifest found for node `ConfigIdentifier { protocol: "test_blockchain", node_type: Node, node_version: "1.2.3" }-test` in `data/test_blockchain/Node`."#,
            cookbook
                .get_download_manifest(id, "test".to_owned())
                .await
                .unwrap_err()
                .to_string()
        );
    }

    #[tokio::test]
    async fn test_get_download_manifest_no_data_version() {
        let mut client = MockClient::new();
        client
            .expect_list()
            .with(eq("bucket"), eq("data/test_blockchain/Node"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "data/test_blockchain/node/invalid/".to_owned(),
                    "data/test_blockchain/node/9.0.1/".to_owned(),
                    "data/test_blockchain/node/0.0.1/".to_owned(),
                    "data/test_blockchain/node/1.2.3/".to_owned(),
                ])
            });
        client
            .expect_list()
            .with(eq("bucket"), eq("data/test_blockchain/Node/1.2.3/test"))
            .once()
            .returning(|_, _| Ok(vec![]));
        client
            .expect_list()
            .with(eq("bucket"), eq("data/test_blockchain/Node/0.0.1/test"))
            .once()
            .returning(|_, _| Ok(vec![]));

        let cookbook = Cookbook::new_with_client(&dummy_config(), client);
        let id = api::ConfigIdentifier {
            protocol: "test_blockchain".to_string(),
            node_type: NodeType::Node.into(),
            node_version: "1.2.3".to_string(),
        };
        assert_eq!(
            r#"No manifest found for node `ConfigIdentifier { protocol: "test_blockchain", node_type: Node, node_version: "1.2.3" }-test` in `data/test_blockchain/Node`."#,
            cookbook
                .get_download_manifest(id, "test".to_owned())
                .await
                .unwrap_err()
                .to_string()
        );
    }

    #[tokio::test]
    async fn test_get_download_manifest_no_manifest_or_invalid() {
        let mut client = MockClient::new();
        client
            .expect_list()
            .with(eq("bucket"), eq("data/test_blockchain/Node"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "data/test_blockchain/node/invalid/".to_owned(),
                    "data/test_blockchain/node/9.0.1/".to_owned(),
                    "data/test_blockchain/node/1.2.3/".to_owned(),
                ])
            });
        client
            .expect_list()
            .with(eq("bucket"), eq("data/test_blockchain/Node/1.2.3/test"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "data/test_blockchain/node/1.2.3/test/invalid/".to_owned(),
                    "data/test_blockchain/node/1.2.3/test/1/".to_owned(),
                    "data/test_blockchain/node/1.2.3/test/2/".to_owned(),
                ])
            });
        client
            .expect_read_file()
            .once()
            .returning(|_, _| Err(Error::Unexpected("no file")));
        client
            .expect_read_file()
            .once()
            .returning(|_, _| Ok("invalid manifest content".to_owned().into_bytes()));

        let cookbook = Cookbook::new_with_client(&dummy_config(), client);
        let id = api::ConfigIdentifier {
            protocol: "test_blockchain".to_string(),
            node_type: NodeType::Node.into(),
            node_version: "1.2.3".to_string(),
        };
        assert_eq!(
            r#"No manifest found for node `ConfigIdentifier { protocol: "test_blockchain", node_type: Node, node_version: "1.2.3" }-test` in `data/test_blockchain/Node`."#,
            cookbook
                .get_download_manifest(id, "test".to_owned())
                .await
                .unwrap_err()
                .to_string()
        );
    }

    #[tokio::test]
    async fn test_get_download_manifest_ok() {
        let mut client = MockClient::new();
        client
            .expect_list()
            .with(eq("bucket"), eq("data/test_blockchain/Node"))
            .once()
            .returning(|_, _| Ok(vec!["data/test_blockchain/node/1.1.1/".to_owned()]));
        client
            .expect_list()
            .with(eq("bucket"), eq("data/test_blockchain/Node/1.1.1/test"))
            .once()
            .returning(|_, _| Ok(vec!["data/test_blockchain/node/1.1.1/test/2/".to_owned()]));
        client
            .expect_read_file()
            .once()
            .returning(|_, _| Ok(r#"{"total_size": 128,"chunks": []}"#.to_owned().into_bytes()));

        let cookbook = Cookbook::new_with_client(&dummy_config(), client);
        let id = api::ConfigIdentifier {
            protocol: "test_blockchain".to_string(),
            node_type: NodeType::Node.into(),
            node_version: "1.2.3".to_string(),
        };
        let manifest = cookbook
            .get_download_manifest(id, "test".to_owned())
            .await
            .unwrap();

        assert_eq!(
            manifest,
            api::DownloadManifest {
                total_size: 128,
                compression: None,
                chunks: vec![],
            }
        );
    }
}
