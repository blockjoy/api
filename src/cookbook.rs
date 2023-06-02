use crate::{
    auth::key_provider::KeyProvider,
    grpc::{api, helpers::required},
};
use anyhow::Context;

pub const RHAI_FILE_NAME: &str = "babel.rhai";
pub const BABEL_IMAGE_NAME: &str = "blockjoy.gz";
pub const KERNEL_NAME: &str = "kernel.gz";
const CHAINS_PREFIX: &str = "chains";

#[derive(Clone)]
pub struct Cookbook {
    pub prefix: String,
    pub bucket: String,
    pub expiration: std::time::Duration,
    pub client: aws_sdk_s3::Client,
    pub engine: std::sync::Arc<rhai::Engine>,
}

impl Cookbook {
    pub fn new_from_env() -> crate::Result<Self> {
        let prefix =
            std::env::var("DIR_CHAINS_PREFIX").unwrap_or_else(|_| CHAINS_PREFIX.to_string());
        let bucket = std::env::var("R2_ROOT").context("`R2_ROOT` not set")?;
        let endpoint = KeyProvider::get_var("R2_URL")?.to_string();
        let expiration_secs = std::env::var("PRESIGNED_URL_EXPIRATION_SECS")
            .context("`PRESIGNED_URL_EXPIRATION_SECS` not set")?
            .parse()
            .context("Can't parse `PRESIGNED_URL_EXPIRATION_SECS`")?;
        let expiration = std::time::Duration::from_secs(expiration_secs);

        let s3_config = aws_sdk_s3::Config::builder().endpoint_url(endpoint).build();
        let client = aws_sdk_s3::Client::from_conf(s3_config);

        let engine = std::sync::Arc::new(rhai::Engine::new());

        Ok(Self {
            prefix,
            bucket,
            expiration,
            client,
            engine,
        })
    }

    pub fn get_networks() {}

    pub async fn read_file(
        &self,
        protocol: &str,
        node_type: &str,
        node_version: &str,
        file: &str,
    ) -> crate::Result<Vec<u8>> {
        let prefix = &self.prefix;
        let file = format!("{prefix}/{protocol}/{node_type}/{node_version}/{file}");
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&file)
            .send()
            .await?;
        let metadata = response.metadata().ok_or_else(required("metadata"))?;
        if !metadata.contains_key("status") {
            let err = format!("File {file} not does not exist");
            return Err(crate::Error::unexpected(err));
        }
        let bytes = response
            .body
            .collect()
            .await
            .with_context(|| format!("Error querying file {file}"))?
            .into_bytes();
        Ok(bytes.to_vec())
    }

    pub async fn read_string(
        &self,
        protocol: &str,
        node_type: &str,
        node_version: &str,
        file: &str,
    ) -> crate::Result<String> {
        let bytes = self
            .read_file(protocol, node_type, node_version, file)
            .await?;
        Ok(String::from_utf8(bytes).context("Invalid utf8")?)
    }

    pub async fn download_url(
        &self,
        protocol: &str,
        node_type: &str,
        node_version: &str,
        file: &str,
    ) -> crate::Result<String> {
        let prefix = &self.prefix;
        let expiration = self.expiration;
        let file = format!("{prefix}/{protocol}/{node_type}/{node_version}/{file}");
        let exp = aws_sdk_s3::presigning::PresigningConfig::expires_in(expiration)
            .with_context(|| format!("Failed to create presigning config from {expiration:?}"))?;
        let url = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&file)
            .presigned(exp)
            .await
            .with_context(|| format!("Failed to create presigned url for {file}"))?
            .uri()
            .to_string();
        Ok(url)
    }

    pub async fn list(
        &self,
        protocol: &str,
        node_type: &str,
    ) -> crate::Result<Vec<api::ConfigIdentifier>> {
        let prefix = &self.prefix;
        let prefix = format!("{prefix}/{protocol}/{node_type}");
        let resp = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&prefix)
            .send()
            .await
            .with_context(|| format!("Cannot `list` for path `{prefix}`"))?;
        let objects = resp.contents().unwrap_or_default();
        let mut identifiers = vec![];
        for obj in objects {
            let Some(key) = obj.key() else { continue };
            let id = api::ConfigIdentifier::from_key(key)?;
            if !identifiers.contains(&id) {
                identifiers.push(id);
            }
        }

        Ok(identifiers.into_iter().collect())
    }

    pub async fn rhai_metadata(
        &self,
        protocol: &str,
        node_type: &str,
        node_version: &str,
    ) -> crate::Result<script::BlockchainMetadata> {
        let script = self
            .read_string(protocol, node_type, node_version, RHAI_FILE_NAME)
            .await?;
        Self::script_to_metadata(&self.engine, &script)
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

// impl std::fmt::Display for api::ConfigIdentifier {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         let bc = self.protocol.to_lowercase();
//         let nt = self.node_type.to_lowercase();
//         let version = self.node_version.to_lowercase();
//         write!(f, "{bc}/{nt}/{version}")
//     }
// }

impl api::ConfigIdentifier {
    fn from_key(key: &str) -> crate::Result<Self> {
        let parts: Vec<&str> = key.split('/').collect();
        let parts: [&str; 5] = parts
            .try_into()
            .map_err(|_| anyhow::anyhow!("{key} is not splittable in 5 `/`-separated parts"))?;
        let [_, protocol, node_type, node_version, _] = parts;
        let id = api::ConfigIdentifier {
            protocol: protocol.to_string(),
            node_type: node_type.to_string(),
            node_version: node_version.to_string(),
            status: 0,
        };
        Ok(id)
    }
}

pub mod script {
    use std::collections::HashMap;

    use crate::grpc::api::network_configuration;

    // Top level struct to hold the blockchain metadata.
    #[derive(Debug, serde::Deserialize)]
    pub struct BlockchainMetadata {
        pub requirements: HardwareRequirements,
        pub nets: HashMap<String, NetConfiguration>,
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

    impl From<NetType> for network_configuration::NetType {
        fn from(value: NetType) -> Self {
            match value {
                NetType::Test => network_configuration::NetType::Test,
                NetType::Main => network_configuration::NetType::Main,
                NetType::Dev => network_configuration::NetType::Dev,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::super::Cookbook;
        use super::*;

        fn get_rhai_contents() -> &'static str {
            r#"
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
            "#
        }

        #[test]
        fn can_deserialize_rhai() -> anyhow::Result<()> {
            let script = get_rhai_contents();
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
