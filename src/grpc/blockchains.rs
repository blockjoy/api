use std::collections::{HashMap, HashSet};

use diesel_async::scoped_futures::ScopedFutureExt;
use futures_util::future::join_all;

use crate::cookbook;
use crate::database::{Conn, ReadConn, Transaction};
use crate::models::blockchain::{Blockchain, BlockchainProperty};
use crate::models::NodeType;
use crate::timestamp::NanosUtc;

use super::api::{self, blockchain_service_server, SupportedNodeProperty};

#[tonic::async_trait]
impl blockchain_service_server::BlockchainService for super::Grpc {
    async fn get(
        &self,
        req: tonic::Request<api::BlockchainServiceGetRequest>,
    ) -> super::Resp<api::BlockchainServiceGetResponse> {
        self.read(|read| get(req, read).scope_boxed()).await
    }

    async fn list(
        &self,
        req: tonic::Request<api::BlockchainServiceListRequest>,
    ) -> super::Resp<api::BlockchainServiceListResponse> {
        self.read(|read| list(req, read).scope_boxed()).await
    }
}

async fn get(
    req: tonic::Request<api::BlockchainServiceGetRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::BlockchainServiceGetResponse> {
    let ReadConn { conn, .. } = read;

    let req: api::BlockchainServiceGetRequest = req.into_inner();
    let id = req.id.parse()?;
    let blockchain = Blockchain::find_by_id(id, conn).await?;
    let resp = api::BlockchainServiceGetResponse {
        blockchain: Some(api::Blockchain::from_model(blockchain, conn).await?),
    };
    Ok(tonic::Response::new(resp))
}

async fn list(
    _req: tonic::Request<api::BlockchainServiceListRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::BlockchainServiceListResponse> {
    let ReadConn { conn, ctx } = read;

    // We need to combine info from two seperate sources: the database and cookbook. Since
    // cookbook is slow, the step where we call it is parallelized.

    // We query the necessary blockchains from the database.
    let blockchains = Blockchain::find_all(conn).await?;

    // This list will contain the dto's that are sent over gRPC after the information from
    // cookbook has been added to them.
    let mut grpc_blockchains = api::Blockchain::from_models(blockchains.clone(), conn).await?;
    let cookbook = ctx.cookbook.clone();

    // Now we need to combine this info with the networks that are stored in the cookbook
    // service. Since we want to do this in parallel, this list will contain a number of futures
    // that each resolve to a list of networks for that blockchain.
    let mut network_identifiers = HashSet::new();
    for blockchain in &blockchains {
        for node_properties in blockchain.properties(conn).await? {
            network_identifiers.insert((
                blockchain.id,
                blockchain.name.clone(),
                node_properties.node_type.to_string(),
                node_properties.version.clone(),
            ));
        }
    }
    let network_futs =
        network_identifiers
            .into_iter()
            .map(|(b_id, b_name, node_type, node_version)| {
                try_get_networks(&cookbook, b_id, b_name, node_type, node_version)
            });
    let networks = join_all(network_futs).await;

    // Now that we have fetched our networks, we have to stuff them into the DTO's. To do this
    // we change the list of tuples into a hashmap, mapping the blockchain_id to the networks
    // belonging to it.
    let mut networks_map: HashMap<uuid::Uuid, Vec<_>> = HashMap::new();
    for (blockchain_id, network) in networks {
        networks_map
            .entry(blockchain_id)
            .or_default()
            .extend(network.into_iter());
    }

    // Now that we have our map, we can simply index it with each blockchain id to get the
    // networks belonging to blockchain.
    for (model, dto) in blockchains.into_iter().zip(grpc_blockchains.iter_mut()) {
        let mut networks = networks_map.get(&model.id).cloned().unwrap_or_default();
        // We deduplicate by name. This is hacky, but only because our representation of the
        // blockchain resource is somewhat false. The list of networks should be represented per
        // blockchain version, but it is instead represented per blockchain. Luuk: fix this.
        networks.sort_by(|n1, n2| n1.name.cmp(&n2.name));
        networks.dedup_by(|n1, n2| n1.name == n2.name);
        dto.networks = networks;
    }

    let resp = api::BlockchainServiceListResponse {
        blockchains: grpc_blockchains,
    };
    Ok(tonic::Response::new(resp))
}

/// This is a helper function for BlockchainService::list. It retrieves the networks for a given set
/// of query parameters, and logs an error when something goes wrong. This behaviour is important,
/// because calls to cookbook sometimes fail and we don't want this whole endpoint to crash when
/// cookbook is having a sad day.
async fn try_get_networks(
    cookbook: &cookbook::Cookbook,
    blockchain_id: uuid::Uuid,
    name: String,
    node_type: String,
    version: String,
) -> (uuid::Uuid, Vec<api::BlockchainNetwork>) {
    // We prepare an error message because we are moving all the arguments used to construct it.
    let err_msg = format!("Could not get networks for {name} {node_type} version {version:?}");

    let networks = match cookbook.rhai_metadata(&name, &node_type, &version).await {
        Ok(meta) => meta
            .nets
            .into_iter()
            .map(|(name, network)| {
                let mut net = api::BlockchainNetwork {
                    name,
                    url: network.url,
                    net_type: 0, // we use a setter
                };
                net.set_net_type(match network.net_type {
                    cookbook::script::NetType::Dev => api::BlockchainNetworkType::Dev,
                    cookbook::script::NetType::Test => api::BlockchainNetworkType::Test,
                    cookbook::script::NetType::Main => api::BlockchainNetworkType::Main,
                });
                net
            })
            .collect(),
        Err(e) => {
            tracing::error!("{err_msg}: {e}");
            vec![]
        }
    };
    (blockchain_id, networks)
}

impl api::Blockchain {
    async fn from_models(models: Vec<Blockchain>, conn: &mut Conn<'_>) -> crate::Result<Vec<Self>> {
        let properties = BlockchainProperty::by_blockchains(&models, conn).await?;
        let mut properties_map: HashMap<uuid::Uuid, Vec<_>> = HashMap::new();
        for property in properties {
            properties_map
                .entry(property.blockchain_id)
                .or_default()
                .push(property);
        }

        models
            .into_iter()
            .map(|model| {
                let properties = properties_map.get(&model.id).cloned().unwrap_or_default();
                Ok(Self {
                    id: model.id.to_string(),
                    name: model.name,
                    // TODO: make this column mandatory
                    description: model.description.unwrap_or_default(),
                    project_url: model.project_url,
                    repo_url: model.repo_url,
                    version: model.version,
                    nodes_types: api::SupportedNodeType::from_models(properties)?,
                    created_at: Some(NanosUtc::from(model.created_at).into()),
                    updated_at: Some(NanosUtc::from(model.updated_at).into()),
                    networks: vec![],
                })
            })
            .collect()
    }

    async fn from_model(model: Blockchain, conn: &mut Conn<'_>) -> crate::Result<Self> {
        Ok(Self::from_models(vec![model], conn).await?[0].clone())
    }
}

impl api::SupportedNodeType {
    fn from_models(models: Vec<BlockchainProperty>) -> crate::Result<Vec<Self>> {
        // First we partition the properties by node type and version.
        let mut properties: HashMap<(NodeType, String), Vec<_>> = HashMap::new();
        for model in models {
            properties
                .entry((model.node_type, model.version.clone()))
                .or_default()
                .push(model);
        }

        properties
            .into_iter()
            .map(|((node_type, version), properties)| {
                let mut props = api::SupportedNodeType {
                    node_type: 0, // We use the setter to set this field for type-safety
                    version,
                    properties: properties
                        .iter()
                        .map(api::SupportedNodeProperty::from_model)
                        .collect(),
                };
                props.set_node_type(api::NodeType::from_model(node_type));
                Ok(props)
            })
            .collect()
    }
}

impl api::SupportedNodeProperty {
    fn from_model(model: &BlockchainProperty) -> Self {
        let mut prop = SupportedNodeProperty {
            name: model.name.clone(),
            default: model.default.clone(),
            ui_type: 0, // We use the setter to set this field for type-safety
            disabled: model.disabled,
            required: model.required,
        };
        prop.set_ui_type(api::UiType::from_model(model.ui_type));
        prop
    }
}
