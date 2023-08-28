use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;
use futures_util::future::join_all;

use crate::cookbook;
use crate::database::{Conn, ReadConn, Transaction};
use crate::models::blockchain::{
    Blockchain, BlockchainId, BlockchainNodeType, BlockchainProperty, BlockchainVersion,
};
use crate::models::NodeType;
use crate::timestamp::NanosUtc;

use super::api::{self, blockchain_service_server};

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
    let ReadConn { conn, ctx } = read;

    let req: api::BlockchainServiceGetRequest = req.into_inner();
    let cookbook = ctx.cookbook.clone();
    let id = req.id.parse()?;
    let blockchain = Blockchain::find_by_id(id, conn).await?;
    let node_types = BlockchainNodeType::by_blockchain(&blockchain, conn).await?;
    let node_type_map: HashMap<_, _> = node_types.into_iter().map(|nt| (nt.id, nt)).collect();
    let versions = BlockchainVersion::by_blockchain(&blockchain, conn).await?;
    let network_futs = versions.iter().map(|version| {
        try_get_networks(
            &cookbook,
            version.id,
            &blockchain.name,
            node_type_map[&version.blockchain_node_type_id].node_type,
            &version.version,
        )
    });
    let version_to_network_map = join_all(network_futs).await.into_iter().collect();
    let blockchain = api::Blockchain::from_model(blockchain, &version_to_network_map, conn).await?;
    let resp = api::BlockchainServiceGetResponse {
        blockchain: Some(blockchain),
    };
    Ok(tonic::Response::new(resp))
}

async fn list(
    _req: tonic::Request<api::BlockchainServiceListRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::BlockchainServiceListResponse> {
    let ReadConn { conn, ctx } = read;
    let cookbook = ctx.cookbook.clone();

    // We need to combine info from two seperate sources: the database and cookbook. Since
    // cookbook is slow, the step where we call it is parallelized.

    // We query the necessary blockchains from the database.
    let blockchains = Blockchain::find_all(conn).await?;

    // Now we need to combine this info with the networks that are stored in the cookbook
    // service. Since we want to do this in parallel, `network_futs` will contain a number of
    // futures that each resolve to a list of networks for that blockchain version.
    let blockchain_map: HashMap<_, _> = blockchains.iter().map(|b| (b.id, b)).collect();
    let node_types = BlockchainNodeType::by_blockchains(&blockchains, conn).await?;
    let node_type_map: HashMap<_, _> = node_types.into_iter().map(|nt| (nt.id, nt)).collect();
    let versions = BlockchainVersion::by_blockchains(&blockchains, conn).await?;
    let network_futs = versions.iter().map(|version| {
        try_get_networks(
            &cookbook,
            version.id,
            &blockchain_map[&version.blockchain_id].name,
            node_type_map[&version.blockchain_node_type_id].node_type,
            &version.version,
        )
    });
    let version_to_network_map = join_all(network_futs).await.into_iter().collect();

    let blockchains =
        api::Blockchain::from_models(blockchains, &version_to_network_map, conn).await?;

    let resp = api::BlockchainServiceListResponse { blockchains };
    Ok(tonic::Response::new(resp))
}

/// This is a helper function for BlockchainService::list. It retrieves the networks for a given set
/// of query parameters, and logs an error when something goes wrong. This behaviour is important,
/// because calls to cookbook sometimes fail and we don't want this whole endpoint to crash when
/// cookbook is having a sad day.
async fn try_get_networks(
    cookbook: &cookbook::Cookbook,
    version_id: uuid::Uuid,
    name: &str,
    node_type: NodeType,
    node_version: &str,
) -> (uuid::Uuid, Vec<api::BlockchainNetwork>) {
    // We prepare an error message because we are moving all the arguments used to construct it.
    let err_msg = format!("Could not get networks for {name} {node_type} version {node_version:?}");

    let networks = match cookbook.rhai_metadata(name, node_type, node_version).await {
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
    (version_id, networks)
}

impl api::Blockchain {
    async fn from_models(
        models: Vec<Blockchain>,
        version_to_network_map: &HashMap<uuid::Uuid, Vec<api::BlockchainNetwork>>,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let mut blockchain_to_node_type_map: HashMap<BlockchainId, Vec<_>> = HashMap::new();
        for node_type in BlockchainNodeType::by_blockchains(&models, conn).await? {
            blockchain_to_node_type_map
                .entry(node_type.blockchain_id)
                .or_default()
                .push(node_type);
        }

        let mut node_type_to_version_map: HashMap<uuid::Uuid, Vec<_>> = HashMap::new();
        for version in BlockchainVersion::by_blockchains(&models, conn).await? {
            node_type_to_version_map
                .entry(version.blockchain_node_type_id)
                .or_default()
                .push(version);
        }

        let mut version_to_property_map: HashMap<uuid::Uuid, Vec<_>> = HashMap::new();
        for property in BlockchainProperty::by_blockchains(&models, conn).await? {
            version_to_property_map
                .entry(property.blockchain_version_id)
                .or_default()
                .push(property);
        }

        models
            .into_iter()
            .map(|model| {
                let node_types = blockchain_to_node_type_map
                    .get(&model.id)
                    .cloned()
                    .unwrap_or_default();
                Ok(Self {
                    id: model.id.to_string(),
                    name: model.name,
                    // TODO: make this column mandatory
                    description: model.description,
                    project_url: model.project_url,
                    repo_url: model.repo_url,
                    node_types: api::BlockchainNodeType::from_models(
                        node_types,
                        &node_type_to_version_map,
                        &version_to_property_map,
                        version_to_network_map,
                    )?,
                    created_at: Some(NanosUtc::from(model.created_at).into()),
                    updated_at: Some(NanosUtc::from(model.updated_at).into()),
                })
            })
            .collect()
    }

    async fn from_model(
        model: Blockchain,
        version_to_network_map: &HashMap<uuid::Uuid, Vec<api::BlockchainNetwork>>,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Self> {
        Ok(Self::from_models(vec![model], version_to_network_map, conn).await?[0].clone())
    }
}

impl api::BlockchainNodeType {
    fn from_models(
        node_types: Vec<BlockchainNodeType>,
        node_type_to_version_map: &HashMap<uuid::Uuid, Vec<BlockchainVersion>>,
        version_to_property_map: &HashMap<uuid::Uuid, Vec<BlockchainProperty>>,
        version_to_network_map: &HashMap<uuid::Uuid, Vec<api::BlockchainNetwork>>,
    ) -> crate::Result<Vec<Self>> {
        node_types
            .into_iter()
            .map(|node_type| {
                let versions = node_type_to_version_map
                    .get(&node_type.id)
                    .cloned()
                    .unwrap_or_default();
                let versions = api::BlockchainVersion::from_models(
                    versions,
                    version_to_property_map,
                    version_to_network_map,
                );
                let mut props = Self {
                    id: node_type.id.to_string(),
                    node_type: 0, // We use the setter to set this field for type-safety
                    versions,
                    description: node_type.description,
                    created_at: Some(NanosUtc::from(node_type.created_at).into()),
                    updated_at: Some(NanosUtc::from(node_type.updated_at).into()),
                };
                props.set_node_type(api::NodeType::from_model(node_type.node_type));
                Ok(props)
            })
            .collect()
    }
}

impl api::BlockchainVersion {
    fn from_models(
        models: Vec<BlockchainVersion>,
        version_to_property_map: &HashMap<uuid::Uuid, Vec<BlockchainProperty>>,
        version_to_network_map: &HashMap<uuid::Uuid, Vec<api::BlockchainNetwork>>,
    ) -> Vec<Self> {
        models
            .into_iter()
            .map(|model| Self {
                id: model.id.to_string(),
                version: model.version,
                description: model.description,
                created_at: Some(NanosUtc::from(model.created_at).into()),
                updated_at: Some(NanosUtc::from(model.updated_at).into()),
                networks: version_to_network_map
                    .get(&model.id)
                    .cloned()
                    .unwrap_or_default(),
                properties: version_to_property_map
                    .get(&model.id)
                    .iter()
                    .flat_map(|props| props.iter())
                    .map(api::BlockchainProperty::from_model)
                    .collect(),
            })
            .collect()
    }
}

impl api::BlockchainProperty {
    fn from_model(model: &BlockchainProperty) -> Self {
        let mut prop = Self {
            name: model.name.clone(),
            display_name: model.display_name.clone(),
            default: model.default.clone(),
            ui_type: 0, // We use the setter to set this field for type-safety
            required: model.required,
        };
        prop.set_ui_type(api::UiType::from_model(model.ui_type));
        prop
    }
}
