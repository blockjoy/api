use std::collections::{HashMap, HashSet};

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use futures_util::future::join_all;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::{error, warn};

use crate::auth::rbac::{BlockchainAdminPerm, BlockchainPerm};
use crate::auth::Authorize;
use crate::cookbook::identifier::Identifier;
use crate::cookbook::script::NetType;
use crate::cookbook::Cookbook;
use crate::database::{Conn, ReadConn, Transaction};
use crate::models::blockchain::{
    Blockchain, BlockchainNodeType, BlockchainNodeTypeId, BlockchainProperty, BlockchainVersion,
    BlockchainVersionId, NodeStats,
};
use crate::models::BlockchainId;
use crate::timestamp::NanosUtc;

use super::api::blockchain_service_server::BlockchainService;
use super::{api, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Blockchain model error: {0}
    Blockchain(#[from] crate::models::blockchain::Error),
    /// Blockchain node type error: {0}
    BlockchainNodeType(#[from] crate::models::blockchain::node_type::Error),
    /// Blockchain version error: {0}
    BlockchainVersion(#[from] crate::models::blockchain::version::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing blockchain version id. This should not happen.
    MissingVersionId,
    /// Missing blockchain version node type. This should not happen.
    MissingVersionNodeType,
    /// Unable to cast node count from i64 to u64 for use in the api: {0}
    NodeCount(std::num::TryFromIntError),
    ///  Unable to cast active node count from i64 to u64 for use in the api: {0}
    NodeCountActive(std::num::TryFromIntError),
    ///  Unable to cast syncing node count from i64 to u64 for use in the api: {0}
    NodeCountSyncing(std::num::TryFromIntError),
    ///  Unable to cast provisioning node count from i64 to u64 for use in the api: {0}
    NodeCountProvisioning(std::num::TryFromIntError),
    ///  Unable to cast ok node count from i64 to u64 for use in the api: {0}
    NodeCountFailed(std::num::TryFromIntError),
    /// Failed to parse BlockchainId: {0}
    ParseId(uuid::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to get blockchain property: {0}
    Property(#[from] crate::models::blockchain::property::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_)
            | MissingVersionId
            | MissingVersionNodeType
            | NodeCount(_)
            | NodeCountActive(_)
            | NodeCountSyncing(_)
            | NodeCountProvisioning(_)
            | NodeCountFailed(_) => Status::internal("Internal error."),
            ParseId(_) => Status::invalid_argument("id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Blockchain(err) => err.into(),
            BlockchainNodeType(err) => err.into(),
            BlockchainVersion(err) => err.into(),
            Property(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl BlockchainService for Grpc {
    async fn get(
        &self,
        req: Request<api::BlockchainServiceGetRequest>,
    ) -> Result<Response<api::BlockchainServiceGetResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta, read).scope_boxed()).await
    }

    async fn list(
        &self,
        req: Request<api::BlockchainServiceListRequest>,
    ) -> Result<Response<api::BlockchainServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }
}

async fn get(
    req: api::BlockchainServiceGetRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainServiceGetResponse, Error> {
    let id = req.id.parse().map_err(Error::ParseId)?;
    let org_id = req
        .org_id
        .as_deref()
        .map(|id| id.parse().map_err(Error::ParseOrgId))
        .transpose()?;
    if let Some(org_id) = org_id {
        read.auth_or_all(&meta, BlockchainAdminPerm::Get, BlockchainPerm::Get, org_id)
            .await?
    } else {
        read.auth_all(&meta, BlockchainAdminPerm::Get).await?
    };
    let blockchain = Blockchain::find_by_id(id, &mut read).await?;

    let node_types = BlockchainNodeType::by_blockchain_id(blockchain.id, &mut read).await?;
    let node_type_map: HashMap<_, _> = node_types.into_iter().map(|nt| (nt.id, nt)).collect();

    let versions = BlockchainVersion::by_blockchain_id(blockchain.id, &mut read).await?;
    let ids = versions
        .iter()
        .map(|version| {
            let node_type = node_type_map
                .get(&version.blockchain_node_type_id)
                .map(|chain_node_type| chain_node_type.node_type)
                .ok_or(Error::MissingVersionNodeType)?;
            let id = Identifier::new(&blockchain.name, node_type, version.version.clone().into());

            Ok((version.id, id))
        })
        .collect::<Result<Vec<(BlockchainVersionId, Identifier)>, Error>>()?;

    let network_futs = ids
        .into_iter()
        .map(|(version_id, id)| try_get_networks(&read.ctx.cookbook, version_id, id));
    let version_to_network_map = join_all(network_futs).await.into_iter().collect();

    let node_stats = Blockchain::node_stats(org_id, &mut read).await?;
    let node_stats = node_stats
        .into_iter()
        .map(|ns| (ns.blockchain_id, ns))
        .collect();

    let blockchain =
        api::Blockchain::from_model(blockchain, &version_to_network_map, &node_stats, &mut read)
            .await?;

    Ok(api::BlockchainServiceGetResponse {
        blockchain: Some(blockchain),
    })
}

async fn list(
    req: api::BlockchainServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainServiceListResponse, Error> {
    let org_id = req
        .org_id
        .as_deref()
        .map(|id| id.parse().map_err(Error::ParseOrgId))
        .transpose()?;

    if let Some(org_id) = org_id {
        read.auth_or_all(
            &meta,
            BlockchainAdminPerm::List,
            BlockchainPerm::List,
            org_id,
        )
        .await?
    } else {
        read.auth_all(&meta, BlockchainAdminPerm::List).await?
    };

    // We need to combine info from two seperate sources: the database and cookbook. Since
    // cookbook is slow, the step where we call it is parallelized.

    // We query the necessary blockchains from the database.
    let blockchains = Blockchain::find_all(&mut read).await?;
    let blockchain_ids: HashSet<_> = blockchains.iter().map(|b| b.id).collect();
    let blockchain_map: HashMap<_, _> = blockchains.iter().map(|b| (b.id, b)).collect();

    // Now we need to combine this info with the networks that are stored in the cookbook
    // service. Since we want to do this in parallel, `network_futs` will contain a number of
    // futures that each resolve to a list of networks for that blockchain version.
    let node_types =
        BlockchainNodeType::by_blockchain_ids(blockchain_ids.clone(), &mut read).await?;
    let node_type_map: HashMap<_, _> = node_types.into_iter().map(|nt| (nt.id, nt)).collect();

    let versions = BlockchainVersion::by_blockchain_ids(blockchain_ids, &mut read).await?;
    let ids = versions
        .iter()
        .map(|version| {
            let protocol = &blockchain_map
                .get(&version.blockchain_id)
                .ok_or(Error::MissingVersionId)?
                .name;

            let node_type = node_type_map
                .get(&version.blockchain_node_type_id)
                .map(|chain_node_type| chain_node_type.node_type)
                .ok_or(Error::MissingVersionNodeType)?;

            let id = Identifier::new(protocol, node_type, version.version.clone().into());

            Ok((version.id, id))
        })
        .collect::<Result<Vec<(BlockchainVersionId, Identifier)>, Error>>()?;

    let network_futs = ids
        .into_iter()
        .map(|(version_id, id)| try_get_networks(&read.ctx.cookbook, version_id, id));
    let version_to_network_map = join_all(network_futs).await.into_iter().collect();

    let node_stats = Blockchain::node_stats(org_id, &mut read).await?;
    let node_stats = node_stats
        .into_iter()
        .map(|ns| (ns.blockchain_id, ns))
        .collect();

    let blockchains =
        api::Blockchain::from_models(blockchains, &version_to_network_map, &node_stats, &mut read)
            .await?;

    Ok(api::BlockchainServiceListResponse { blockchains })
}

/// This is a helper function for `BlockchainService::list`.
///
/// It retrieves the networks for a given set of query parameters, and logs an
/// error when something goes wrong. This behaviour is important because calls
/// to cookbook sometimes fail and we don't want this whole endpoint to crash
/// when cookbook is having a sad day.
async fn try_get_networks(
    cookbook: &Cookbook,
    version_id: BlockchainVersionId,
    id: Identifier,
) -> (BlockchainVersionId, Vec<api::BlockchainNetwork>) {
    let metadata = match cookbook.rhai_metadata(&id).await {
        Ok(meta) => meta,
        Err(err) => {
            warn!("Could not get networks for {id:?}: {err}");
            return (version_id, vec![]);
        }
    };

    let networks = metadata
        .nets
        .into_iter()
        .map(|(name, network)| {
            let mut net = api::BlockchainNetwork {
                name,
                url: network.url,
                net_type: 0, // we use a setter
            };
            net.set_net_type(match network.net_type {
                NetType::Dev => api::BlockchainNetworkType::Dev,
                NetType::Test => api::BlockchainNetworkType::Test,
                NetType::Main => api::BlockchainNetworkType::Main,
            });
            net
        })
        .collect();

    (version_id, networks)
}

impl api::Blockchain {
    async fn from_models(
        models: Vec<Blockchain>,
        version_to_network_map: &HashMap<BlockchainVersionId, Vec<api::BlockchainNetwork>>,
        node_stats: &HashMap<BlockchainId, NodeStats>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let ids: HashSet<_> = models.iter().map(|blockchain| blockchain.id).collect();

        let mut blockchain_to_node_type_map: HashMap<_, Vec<_>> = HashMap::new();
        for node_type in BlockchainNodeType::by_blockchain_ids(ids.clone(), conn).await? {
            blockchain_to_node_type_map
                .entry(node_type.blockchain_id)
                .or_default()
                .push(node_type);
        }

        let mut node_type_to_version_map: HashMap<_, Vec<_>> = HashMap::new();
        for version in BlockchainVersion::by_blockchain_ids(ids.clone(), conn).await? {
            node_type_to_version_map
                .entry(version.blockchain_node_type_id)
                .or_default()
                .push(version);
        }

        let mut version_to_property_map: HashMap<_, Vec<_>> = HashMap::new();
        for property in BlockchainProperty::by_blockchain_ids(ids, conn).await? {
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
                    node_count: Some(
                        node_stats
                            .get(&model.id)
                            .map(|stat| stat.node_count)
                            .unwrap_or_default()
                            .try_into()
                            .map_err(Error::NodeCount)?,
                    ),
                    node_count_active: Some(
                        node_stats
                            .get(&model.id)
                            .map(|stat| stat.node_count_active)
                            .unwrap_or_default()
                            .try_into()
                            .map_err(Error::NodeCountActive)?,
                    ),
                    node_count_syncing: Some(
                        node_stats
                            .get(&model.id)
                            .map(|stat| stat.node_count_syncing)
                            .unwrap_or_default()
                            .try_into()
                            .map_err(Error::NodeCountSyncing)?,
                    ),
                    node_count_provisioning: Some(
                        node_stats
                            .get(&model.id)
                            .map(|stat| stat.node_count_provisioning)
                            .unwrap_or_default()
                            .try_into()
                            .map_err(Error::NodeCountProvisioning)?,
                    ),
                    node_count_failed: Some(
                        node_stats
                            .get(&model.id)
                            .map(|stat| stat.node_count_failed)
                            .unwrap_or_default()
                            .try_into()
                            .map_err(Error::NodeCountFailed)?,
                    ),
                })
            })
            .collect()
    }

    async fn from_model(
        model: Blockchain,
        version_to_network_map: &HashMap<BlockchainVersionId, Vec<api::BlockchainNetwork>>,
        node_stats: &HashMap<BlockchainId, NodeStats>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let chains =
            Self::from_models(vec![model], version_to_network_map, node_stats, conn).await?;
        Ok(chains[0].clone())
    }
}

impl api::BlockchainNodeType {
    fn from_models(
        node_types: Vec<BlockchainNodeType>,
        node_type_to_version_map: &HashMap<BlockchainNodeTypeId, Vec<BlockchainVersion>>,
        version_to_property_map: &HashMap<BlockchainVersionId, Vec<BlockchainProperty>>,
        version_to_network_map: &HashMap<BlockchainVersionId, Vec<api::BlockchainNetwork>>,
    ) -> Result<Vec<Self>, Error> {
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
        version_to_property_map: &HashMap<BlockchainVersionId, Vec<BlockchainProperty>>,
        version_to_network_map: &HashMap<BlockchainVersionId, Vec<api::BlockchainNetwork>>,
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
        let mut prop = api::BlockchainProperty {
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
