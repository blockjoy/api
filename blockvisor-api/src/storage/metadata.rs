use serde::Deserialize;
use std::collections::HashMap;

use displaydoc::Display;
use rhai::Engine;
use thiserror::Error;

use crate::grpc::common;

use super::image::ImageId;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to compile script from ImageId `{0:?}`: {1}
    CompileScript(ImageId, rhai::ParseError),
    /// Invalid rhai script: {0}
    InvalidScript(Box<rhai::EvalAltResult>),
    /// NetType is unspecified.
    NetTypeUnspecified,
    /// No metadata in rhai script.
    NoMetadata,
}

#[derive(Debug, Deserialize)]
pub struct BlockchainMetadata {
    pub requirements: HardwareRequirements,
    #[serde(alias = "nets")]
    pub networks: HashMap<String, NetworkConfig>,
    pub babel_config: Option<BabelConfig>,
}

impl BlockchainMetadata {
    pub fn from_script(engine: &Engine, script: &str, image: &ImageId) -> Result<Self, Error> {
        let (_, _, dynamic) = engine
            .compile(script)
            .map_err(|err| Error::CompileScript(image.clone(), err))?
            .iter_literal_variables(true, false)
            .find(|&(name, _, _)| name == "METADATA")
            .ok_or(Error::NoMetadata)?;

        rhai::serde::from_dynamic(&dynamic).map_err(Error::InvalidScript)
    }
}

#[derive(Debug, Deserialize)]
pub struct HardwareRequirements {
    pub vcpu_count: u32,
    pub mem_size_mb: u64,
    pub disk_size_gb: u64,
}

impl From<common::HardwareRequirements> for HardwareRequirements {
    fn from(requirements: common::HardwareRequirements) -> Self {
        HardwareRequirements {
            vcpu_count: requirements.vcpu_count,
            mem_size_mb: requirements.mem_size_mb,
            disk_size_gb: requirements.disk_size_gb,
        }
    }
}

impl From<HardwareRequirements> for common::HardwareRequirements {
    fn from(requirements: HardwareRequirements) -> Self {
        common::HardwareRequirements {
            vcpu_count: requirements.vcpu_count,
            mem_size_mb: requirements.mem_size_mb,
            disk_size_gb: requirements.disk_size_gb,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct NetworkConfig {
    #[serde(default)]
    pub name: String,
    pub url: String,
    pub net_type: NetType,
    #[serde(flatten)]
    pub metadata: HashMap<String, String>,
}

impl TryFrom<common::NetworkConfig> for NetworkConfig {
    type Error = Error;

    fn try_from(config: common::NetworkConfig) -> Result<Self, Self::Error> {
        let net_type = config.net_type().try_into()?;

        Ok(NetworkConfig {
            name: config.name,
            url: config.url,
            net_type,
            metadata: config.metadata,
        })
    }
}

impl From<NetworkConfig> for common::NetworkConfig {
    fn from(config: NetworkConfig) -> Self {
        common::NetworkConfig {
            name: config.name,
            url: config.url.to_string(),
            net_type: common::NetType::from(config.net_type).into(),
            metadata: config.metadata,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetType {
    Dev,
    Test,
    Main,
}

impl TryFrom<common::NetType> for NetType {
    type Error = Error;

    fn try_from(net_type: common::NetType) -> Result<Self, Self::Error> {
        match net_type {
            common::NetType::Unspecified => Err(Error::NetTypeUnspecified),
            common::NetType::Dev => Ok(NetType::Dev),
            common::NetType::Test => Ok(NetType::Test),
            common::NetType::Main => Ok(NetType::Main),
        }
    }
}

impl From<NetType> for common::NetType {
    fn from(net_type: NetType) -> Self {
        match net_type {
            NetType::Dev => common::NetType::Dev,
            NetType::Test => common::NetType::Test,
            NetType::Main => common::NetType::Main,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BabelConfig {
    pub data_directory_mount_point: Option<String>,
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    #[cfg(test)]
    use crate::{models::NodeType, storage::image::ImageId};

    #[cfg(test)]
    use super::*;

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

    #[test]
    fn can_deserialize_rhai() {
        let engine = Engine::new();
        let id = ImageId::new("test", NodeType::Node, "1.2.3".to_string().into());
        let meta = BlockchainMetadata::from_script(&engine, TEST_SCRIPT, &id).unwrap();

        assert_eq!(meta.requirements.vcpu_count, 1);
        assert_eq!(meta.requirements.mem_size_mb, 8192);
        assert_eq!(meta.requirements.disk_size_gb, 10);

        let mainnet = meta.networks.get("mainnet").unwrap();
        let sepolia = meta.networks.get("sepolia").unwrap();
        let goerli = meta.networks.get("goerli").unwrap();

        assert_eq!(mainnet.net_type, NetType::Main);
        assert_eq!(sepolia.net_type, NetType::Test);
        assert_eq!(goerli.net_type, NetType::Test);

        assert_eq!(mainnet.url, "https://rpc.ankr.com/eth");
        assert_eq!(sepolia.url, "https://rpc.sepolia.dev");
        assert_eq!(goerli.url, "https://goerli.prylabs.net");

        assert_eq!(
            mainnet.metadata.get("beacon_nodes_csv").unwrap(),
            "http://beacon01.mainnet.eth.blockjoy.com,http://beacon02.mainnet.eth.blockjoy.com?123"
        );
        assert_eq!(
            sepolia.metadata.get("beacon_nodes_csv").unwrap(),
            "http://beacon01.sepolia.eth.blockjoy.com,http://beacon02.sepolia.eth.blockjoy.com?456"
        );
        assert_eq!(
            goerli.metadata.get("beacon_nodes_csv").unwrap(),
            "http://beacon01.goerli.eth.blockjoy.com,http://beacon02.goerli.eth.blockjoy.com?789"
        );

        assert_eq!(mainnet.metadata.get("param_a").unwrap(), "value_a");
        assert_eq!(sepolia.metadata.get("param_b").unwrap(), "value_b");
        assert_eq!(goerli.metadata.get("param_c").unwrap(), "value_c");
    }
}
