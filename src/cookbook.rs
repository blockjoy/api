use crate::auth::key_provider::KeyProvider;
use crate::grpc::api;
use crate::{Error, Result as ApiResult};
use anyhow::{anyhow, Context};
use cookbook_grpc::cook_book_service_client;
use tonic::Request;

pub const COOKBOOK_URL: &str = "COOKBOOK_URL";
pub const COOKBOOK_TOKEN: &str = "COOKBOOK_TOKEN";

#[derive(Debug, Clone, Copy)]
pub struct HardwareRequirements {
    #[allow(unused)]
    pub(crate) vcpu_count: i64,
    pub(crate) mem_size_mb: i64,
    pub(crate) disk_size_gb: i64,
}

#[derive(Clone, Debug)]
pub struct BlockchainNetwork {
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) network_type: api::BlockchainNetworkType,
}

pub mod cookbook_grpc {
    tonic::include_proto!("blockjoy.api.v1.babel");
}

pub async fn get_hw_requirements(
    protocol: String,
    node_type: String,
    node_version: String,
) -> ApiResult<HardwareRequirements> {
    let id = cookbook_grpc::ConfigIdentifier {
        protocol,
        node_type,
        node_version,
        status: 1,
    };
    let cb_url = KeyProvider::get_var(COOKBOOK_URL).map_err(Error::Key)?;
    let cb_token = base64::encode(KeyProvider::get_var(COOKBOOK_TOKEN)?);
    let mut client = cook_book_service_client::CookBookServiceClient::connect(cb_url)
        .await
        .map_err(|e| Error::UnexpectedError(anyhow!("Can't connect to cookbook: {e}")))?;
    let mut request = Request::new(id);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {cb_token}")
            .parse()
            .map_err(|e| Error::UnexpectedError(anyhow!("Can't set cookbook auth header: {e}")))?,
    );

    let response = client.requirements(request).await?;
    let inner = response.into_inner();

    Ok(HardwareRequirements {
        vcpu_count: inner.vcpu_count,
        mem_size_mb: inner.mem_size_mb,
        disk_size_gb: inner.disk_size_gb,
    })
}

/// Given a protocol/blockchain name (i.e. "ethereum"), node_type and node_version, returns a list
/// of supported networks. These are things like "mainnet" and "goerli". If no version is provided,
/// we default to using the latest version.
pub async fn get_networks(
    protocol: String,
    node_type: String,
    node_version: String,
) -> ApiResult<Vec<BlockchainNetwork>> {
    let id = cookbook_grpc::ConfigIdentifier {
        protocol,
        node_type,
        node_version,
        status: 1,
    };
    let cb_url = KeyProvider::get_var(COOKBOOK_URL).map_err(Error::Key)?;
    let cb_token = base64::encode(KeyProvider::get_var(COOKBOOK_TOKEN)?);
    let mut client = cook_book_service_client::CookBookServiceClient::connect(cb_url)
        .await
        .with_context(|| "Can't connect to cookbook")?;
    let mut request = Request::new(id);

    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {cb_token}")
            .parse()
            .with_context(|| "Can't set cookbook auth header")?,
    );

    let response = client.net_configurations(request).await?;
    let inner = response.into_inner();

    inner
        .configurations
        .into_iter()
        .map(|c| c.try_into())
        .collect()
}

impl From<BlockchainNetwork> for api::BlockchainNetwork {
    fn from(value: BlockchainNetwork) -> Self {
        Self {
            name: value.name,
            url: value.url,
            net_type: value.network_type.into(),
        }
    }
}

impl TryFrom<api::BlockchainNetwork> for BlockchainNetwork {
    type Error = Error;

    fn try_from(value: api::BlockchainNetwork) -> crate::Result<Self> {
        Ok(Self {
            name: value.name,
            url: value.url,
            network_type: api::BlockchainNetworkType::from_i32(value.net_type)
                .ok_or_else(|| anyhow!("Unknown network type: {}", value.net_type))?,
        })
    }
}

impl TryFrom<cookbook_grpc::NetworkConfiguration> for BlockchainNetwork {
    type Error = Error;

    fn try_from(value: cookbook_grpc::NetworkConfiguration) -> crate::Result<Self> {
        Ok(Self {
            name: value.name.clone(),
            url: value.url.clone(),
            network_type: api::BlockchainNetworkType::from_i32(value.net_type)
                .ok_or_else(|| anyhow!("Unknown network type: {}", value.net_type))?,
        })
    }
}
