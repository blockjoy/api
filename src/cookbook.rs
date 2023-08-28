use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use aws_sdk_s3::config::Credentials;

use crate::config;
use crate::grpc::api;
use crate::models::NodeType;

pub const RHAI_FILE_NAME: &str = "babel.rhai";
pub const BABEL_IMAGE_NAME: &str = "blockjoy.gz";
pub const KERNEL_NAME: &str = "kernel.gz";
pub const BUNDLE_NAME: &str = "bvd-bundle.tgz";

#[derive(Clone)]
pub struct Cookbook {
    pub prefix: String,
    pub bucket: String,
    pub bundle_bucket: String,
    pub kernel_bucket: String,
    pub expiration: std::time::Duration,
    pub client: Arc<dyn Client>,
    pub engine: std::sync::Arc<rhai::Engine>,
}

#[tonic::async_trait]
pub trait Client: Send + Sync {
    async fn read_file(&self, bucket: &str, path: &str) -> crate::Result<Vec<u8>>;

    async fn read_string(&self, bucket: &str, path: &str) -> crate::Result<String> {
        let bytes = self.read_file(bucket, path).await?;
        let s = std::str::from_utf8(&bytes).with_context(|| format!("Invalid utf8: {bytes:?}"))?;
        Ok(s.to_owned())
    }

    async fn download_url(
        &self,
        bucket: &str,
        path: &str,
        expiration: Duration,
    ) -> crate::Result<String>;

    async fn list(&self, bucket: &str, path: &str) -> crate::Result<Vec<String>>;
}

#[tonic::async_trait]
impl Client for aws_sdk_s3::Client {
    async fn read_file(&self, bucket: &str, path: &str) -> crate::Result<Vec<u8>> {
        let path = path.to_lowercase();
        let response = self
            .get_object()
            .bucket(bucket)
            .key(&path)
            .send()
            .await
            .with_context(|| format!("Can't read file `{bucket}:{path}`"))?;
        let bytes = response
            .body
            .collect()
            .await
            .with_context(|| format!("Error querying file `{bucket}:{path}`"))?
            .into_bytes();
        Ok(bytes.to_vec())
    }

    async fn download_url(
        &self,
        bucket: &str,
        path: &str,
        expiration: Duration,
    ) -> crate::Result<String> {
        let path = path.to_lowercase();
        let exp = aws_sdk_s3::presigning::PresigningConfig::expires_in(expiration)
            .with_context(|| format!("Failed to create presigning config from {expiration:?}"))?;
        let url = self
            .get_object()
            .bucket(bucket)
            .key(&path)
            .presigned(exp)
            .await
            .with_context(|| format!("Failed to create presigned url for {path}"))?
            .uri()
            .to_string();
        Ok(url)
    }

    async fn list(&self, bucket: &str, path: &str) -> crate::Result<Vec<String>> {
        let path = path.to_lowercase();
        let resp = self
            .list_objects_v2()
            .bucket(bucket)
            .prefix(&path)
            .send()
            .await
            .with_context(|| format!("Cannot `list` for path `{path}`"))?;
        let files = resp
            .contents()
            .unwrap_or_default()
            .iter()
            .filter_map(|object| object.key().map(|key| key.to_owned()))
            .collect();
        Ok(files)
    }
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

    /// Creates a new instance of `Cookbook` using the `client` parameter as the storage
    /// implementation.
    pub fn new_with_client(
        config: &config::cookbook::Config,
        client: impl Client + 'static,
    ) -> Self {
        let engine = std::sync::Arc::new(rhai::Engine::new());

        Self {
            prefix: config.dir_chains_prefix.clone(),
            bucket: config.r2_bucket.clone(),
            bundle_bucket: config.bundle_bucket.clone(),
            kernel_bucket: config.kernel_bucket.clone(),
            expiration: config.presigned_url_expiration.to_std(),
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
    ) -> crate::Result<Vec<u8>> {
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
    ) -> crate::Result<String> {
        let path = format!(
            "{prefix}/{protocol}/{node_type}/{node_version}/{file}",
            prefix = self.prefix,
        );
        self.client
            .download_url(&self.bucket, &path, self.expiration)
            .await
    }

    pub async fn download_url_kernel(&self, version: &str) -> crate::Result<String> {
        let path = format!("{version}/{KERNEL_NAME}");
        self.client
            .download_url(&self.kernel_bucket, &path, self.expiration)
            .await
    }

    pub async fn bundle_download_url(&self, version: &str) -> crate::Result<String> {
        let path = format!("{version}/{BUNDLE_NAME}");
        self.client
            .download_url(&self.bundle_bucket, &path, self.expiration)
            .await
    }

    pub async fn list(
        &self,
        protocol: &str,
        node_type: NodeType,
    ) -> crate::Result<Vec<api::ConfigIdentifier>> {
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
        let mut idents: HashMap<_, _> = HashMap::new();
        for ident in self.client.list(&self.bucket, &path).await?.iter() {
            let ident = api::ConfigIdentifier::from_key(ident)?;
            idents.insert(ident.node_type(), ident);
        }
        Ok(idents.into_values().collect())
    }

    pub async fn list_bundles(&self) -> crate::Result<Vec<api::BundleIdentifier>> {
        Ok(self
            .client
            .list(&self.bundle_bucket, "")
            .await?
            .iter()
            .flat_map(api::BundleIdentifier::from_key)
            .collect())
    }

    pub async fn list_kernels(&self) -> crate::Result<Vec<api::KernelIdentifier>> {
        Ok(self
            .client
            .list(&self.kernel_bucket, "")
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
    ) -> crate::Result<script::BlockchainMetadata> {
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
    ) -> crate::Result<api::DownloadManifest> {
        let path = format!(
            "{}/{}/{}/{}/{}/manifest.json",
            self.prefix,
            id.protocol,
            id.node_type().into_model(),
            id.node_version,
            network
        );
        let mut manifest: manifest::DownloadManifest =
            serde_json::from_str(&self.client.read_string(&self.bucket, &path).await?)?;
        for chunk in manifest.chunks.iter_mut() {
            chunk.url = self
                .client
                .download_url(&self.bucket, &chunk.key, self.expiration)
                .await?;
        }
        Ok(manifest.try_into()?)
    }

    fn script_to_metadata(
        engine: &rhai::Engine,
        script: &str,
    ) -> crate::Result<script::BlockchainMetadata> {
        let (_, _, dynamic) = engine
            .compile(script)
            .context("Can't compile script")?
            .iter_literal_variables(true, false)
            .find(|&(name, _, _)| name == "METADATA")
            .ok_or_else(|| crate::Error::unexpected("Invalid rhai script: no METADATA present!"))?;
        let meta: script::BlockchainMetadata = rhai::serde::from_dynamic(&dynamic)
            .context("Invalid Rhai script - failed to deserialize METADATA")?;
        Ok(meta)
    }
}

impl api::ConfigIdentifier {
    fn from_key(key: impl AsRef<str>) -> crate::Result<Self> {
        // We want to parse a `ConfigIdentifier` from a file path. This file path looks like this:
        // `/prefix/ethereum/validator/0.0.3/babel.rhai`. This means that we need to extract the
        // relevant parts by `/`-splitting the path.
        let key = key.as_ref();
        let parts: Vec<&str> = key.split('/').collect();
        let [_, protocol, node_type, node_version, ..] = &parts[..] else {
            return Err(
                anyhow!("{key} is not splittable in at least 4 `/`-separated parts").into(),
            );
        };
        let node_type: NodeType = node_type.parse()?;
        let id = api::ConfigIdentifier {
            protocol: protocol.to_string(),
            node_type: api::NodeType::from_model(node_type).into(),
            node_version: node_version.to_string(),
        };
        Ok(id)
    }
}

impl api::BundleIdentifier {
    fn from_key(key: impl AsRef<str>) -> crate::Result<Self> {
        // "0.1.0/bvd-bundle.tgz"
        let key = key.as_ref();
        // "0.1.0"
        let Some(version) = key.strip_suffix(&format!("/{BUNDLE_NAME}")) else {
            return Err(anyhow!("File name should end in /{BUNDLE_NAME}, but is {key}").into());
        };
        semver::Version::parse(version).context("cannot parse version")?;
        let id = Self {
            version: version.to_owned(),
        };
        Ok(id)
    }
}

impl api::KernelIdentifier {
    fn from_key(key: impl AsRef<str>) -> crate::Result<Self> {
        // "5.10.174-build.1+fc.ufw/kernel.gz"
        let key = key.as_ref();
        // "5.10.174-build.1+fc.ufw"
        let Some(version) = key.strip_suffix(&format!("/{KERNEL_NAME}")) else {
            return Err(anyhow!("File name should end in /{KERNEL_NAME}, but is {key}").into());
        };
        let id = Self {
            version: version.to_owned(),
        };
        Ok(id)
    }
}

pub mod script {
    use crate::grpc::api;
    use std::collections::HashMap;

    // Top level struct to hold the blockchain metadata.
    #[derive(Debug, serde::Deserialize)]
    pub struct BlockchainMetadata {
        pub requirements: HardwareRequirements,
        pub nets: HashMap<String, NetConfiguration>,
        pub babel_config: Option<BabelConfig>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct HardwareRequirements {
        pub vcpu_count: u64,
        pub mem_size_mb: u64,
        pub disk_size_gb: u64,
    }

    #[derive(Debug, Clone, PartialEq, serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum NetType {
        Dev,
        Test,
        Main,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct NetConfiguration {
        pub url: String,
        pub net_type: NetType,
        #[serde(flatten)]
        pub meta: HashMap<String, String>,
    }

    impl From<NetType> for api::NetType {
        fn from(value: NetType) -> Self {
            match value {
                NetType::Test => api::NetType::Test,
                NetType::Main => api::NetType::Main,
                NetType::Dev => api::NetType::Dev,
            }
        }
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct BabelConfig {
        pub data_directory_mount_point: Option<String>,
    }

    pub const TEST_SCRIPT: &str = r#"
        const METADATA = #{
            // comments are allowed
            min_babel_version: "0.0.9",
            node_version: "node_v",
            protocol: "proto",
            node_type: "n_type",
            description: "node description",
            requirements: #{
                vcpu_count: 1,
                mem_size_mb: 8192,
                disk_size_gb: 10,
                more: 0,
            },
            nets: #{
                mainnet: #{
                    url: "https://rpc.ankr.com/eth",
                    net_type: "main",
                    beacon_nodes_csv: "http://beacon01.mainnet.eth.blockjoy.com,http://beacon02.mainnet.eth.blockjoy.com?123",
                    param_a: "value_a",
                },
                sepolia: #{
                    url: "https://rpc.sepolia.dev",
                    net_type: "test",
                    beacon_nodes_csv: "http://beacon01.sepolia.eth.blockjoy.com,http://beacon02.sepolia.eth.blockjoy.com?456",
                    param_b: "value_b",
                },
                goerli: #{
                    url: "https://goerli.prylabs.net",
                    net_type: "test",
                    beacon_nodes_csv: "http://beacon01.goerli.eth.blockjoy.com,http://beacon02.goerli.eth.blockjoy.com?789",
                    param_c: "value_c",
                },
            },
            babel_config: #{
                data_directory_mount_point: "/mnt/data/",
                log_buffer_capacity_ln: 1024,
                swap_size_mb: 1024,
            },
            firewall: #{
                enabled: true,
                default_in: "deny",
                default_out: "allow",
                rules: [
                    #{
                        name: "Rule A",
                        action: "allow",
                        direction: "in",
                        protocol: "tcp",
                        ips: "192.168.0.1/24",
                        ports: [77, 1444, 8080],
                    },
                    #{
                        name: "Rule B",
                        action: "deny",
                        direction: "out",
                        protocol: "udp",
                        ips: "192.167.0.1/24",
                        ports: [77],
                    },
                    #{
                        name: "Rule C",
                        action: "reject",
                        direction: "out",
                        ips: "192.169.0.1/24",
                        ports: [],
                    },
                ],
            },
            keys: #{
                key_a_name: "key A Value",
                key_B_name: "key B Value",
                key_X_name: "X",
                "*": "/*"
            },
        };
        fn some_function() {}
    "#;

    #[cfg(test)]
    mod tests {
        use super::super::Cookbook;
        use super::*;

        #[test]
        fn can_deserialize_rhai() -> anyhow::Result<()> {
            let script = super::TEST_SCRIPT;
            let engine = rhai::Engine::new();
            let config = Cookbook::script_to_metadata(&engine, script)?;

            assert_eq!(config.requirements.vcpu_count, 1);
            assert_eq!(config.requirements.mem_size_mb, 8192);
            assert_eq!(config.requirements.disk_size_gb, 10);

            let mainnet = config.nets.get("mainnet").unwrap();
            let sepolia = config.nets.get("sepolia").unwrap();
            let goerli = config.nets.get("goerli").unwrap();

            assert_eq!(mainnet.net_type, NetType::Main);
            assert_eq!(sepolia.net_type, NetType::Test);
            assert_eq!(goerli.net_type, NetType::Test);

            assert_eq!(mainnet.url, "https://rpc.ankr.com/eth");
            assert_eq!(sepolia.url, "https://rpc.sepolia.dev");
            assert_eq!(goerli.url, "https://goerli.prylabs.net");

            assert_eq!(
                mainnet.meta.get("beacon_nodes_csv").unwrap(),
                "http://beacon01.mainnet.eth.blockjoy.com,http://beacon02.mainnet.eth.blockjoy.com?123"
            );
            assert_eq!(
                sepolia.meta.get("beacon_nodes_csv").unwrap(),
                "http://beacon01.sepolia.eth.blockjoy.com,http://beacon02.sepolia.eth.blockjoy.com?456"
            );
            assert_eq!(
                goerli.meta.get("beacon_nodes_csv").unwrap(),
                "http://beacon01.goerli.eth.blockjoy.com,http://beacon02.goerli.eth.blockjoy.com?789"
            );

            assert_eq!(mainnet.meta.get("param_a").unwrap(), "value_a");
            assert_eq!(sepolia.meta.get("param_b").unwrap(), "value_b");
            assert_eq!(goerli.meta.get("param_c").unwrap(), "value_c");

            Ok(())
        }
    }
}

/// WARNING! These structures were brutally copy pasted
/// from BlockvisorD repository (blockvisor/babel_api/src/engine.rs)
/// To be removed once switched to monorepo.
mod manifest {
    use crate::grpc::api;
    use anyhow::bail;
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct FileLocation {
        pub path: PathBuf,
        pub pos: u64,
        pub size: u64,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum Checksum {
        Sha1([u8; 20]),
        Sha256([u8; 32]),
        Blake3([u8; 32]),
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct Chunk {
        pub key: String,
        pub url: String,
        pub checksum: Checksum,
        pub size: u64,
        pub destinations: Vec<FileLocation>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum Compression {}

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct DownloadManifest {
        pub total_size: u64,
        pub compression: Option<Compression>,
        pub chunks: Vec<Chunk>,
    }

    impl TryInto<api::Compression> for Compression {
        type Error = anyhow::Error;
        fn try_into(self) -> Result<api::Compression, Self::Error> {
            bail!("Invalid compression type");
        }
    }

    impl TryInto<api::DownloadManifest> for DownloadManifest {
        type Error = anyhow::Error;
        fn try_into(self) -> Result<api::DownloadManifest, Self::Error> {
            let compression: Option<api::Compression> = if let Some(compression) = self.compression
            {
                Some(compression.try_into()?)
            } else {
                None
            };
            let chunks = self
                .chunks
                .into_iter()
                .map(|value| {
                    let (checksum_type, checksum) = match value.checksum {
                        Checksum::Sha1(value) => (api::ChecksumType::Sha1, value.to_vec()),
                        Checksum::Sha256(value) => (api::ChecksumType::Sha256, value.to_vec()),
                        Checksum::Blake3(value) => (api::ChecksumType::Blake3, value.to_vec()),
                    };
                    let destinations = value
                        .destinations
                        .into_iter()
                        .map(|value| {
                            Ok(api::FileLocation {
                                path: value.path.to_string_lossy().to_string(),
                                position_bytes: value.pos,
                                size_bytes: value.size,
                            })
                        })
                        .collect::<Result<Vec<_>, Self::Error>>()?;
                    Ok(api::Chunk {
                        key: value.key,
                        url: value.url,
                        checksum_type: checksum_type.into(),
                        checksum,
                        size: value.size,
                        destinations,
                    })
                })
                .collect::<Result<Vec<_>, Self::Error>>()?;
            Ok(api::DownloadManifest {
                total_size: self.total_size,
                compression: compression.map(|value| value.into()),
                chunks,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
