use std::collections::{HashMap, HashSet};

use chrono::Utc;
use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use futures_util::future::join_all;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::{error, warn};

use crate::auth::rbac::{BlockchainAdminPerm, BlockchainPerm};
use crate::auth::resource::NodeId;
use crate::auth::Authorize;
use crate::cookbook::image::Image;
use crate::cookbook::Cookbook;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::models::blockchain::{
    Blockchain, BlockchainId, BlockchainNodeType, BlockchainNodeTypeId, BlockchainProperty,
    BlockchainVersion, BlockchainVersionId, NewBlockchainNodeType, NewProperty, NewVersion,
    NodeStats,
};
use crate::models::command::NewCommand;
use crate::models::node::{NewNodeLog, Node, NodeLogEvent, NodeType, NodeVersion};
use crate::models::{Command, CommandType};
use crate::util::{HashVec, NanosUtc};

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
    /// Blockchain failed to get cookbook networks for `{0:?}`: {1}
    CookbookNetworks(Image, crate::cookbook::Error),
    /// Blockchain command failed: {0}
    Command(#[from] crate::models::command::Error),
    /// Blockchain command failed: {0}
    CommandGrpc(#[from] crate::grpc::command::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing blockchain id. This should not happen.
    MissingId,
    /// Missing `api::Blockchain` model output. This should not happen.
    MissingModel,
    /// Missing BlockchainVersionId in networks. This should not happen.
    MissingNetworksVersion,
    /// Missing blockchain node type. This should not happen.
    MissingNodeType,
    /// Blockchain node error: {0}
    Node(#[from] crate::models::node::Error),
    /// Unable to cast node count from i64 to u64: {0}
    NodeCount(std::num::TryFromIntError),
    /// Unable to cast active node count from i64 to u64: {0}
    NodeCountActive(std::num::TryFromIntError),
    /// Unable to cast syncing node count from i64 to u64: {0}
    NodeCountSyncing(std::num::TryFromIntError),
    /// Unable to cast provisioning node count from i64 to u64: {0}
    NodeCountProvisioning(std::num::TryFromIntError),
    /// Unable to cast failed node count from i64 to u64: {0}
    NodeCountFailed(std::num::TryFromIntError),
    /// Blockchain node log error: {0}
    NodeLog(#[from] crate::models::node::log::Error),
    /// The node type already exists.
    NodeTypeExists,
    /// Blockchain node type error: {0}
    NodeType(#[from] crate::models::node::node_type::Error),
    /// Failed to parse BlockchainId: {0}
    ParseId(uuid::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Blockchain property error: {0}
    Property(#[from] crate::models::blockchain::property::Error),
    /// Unknown NodeType: {0}
    UnknownNodeType(prost::DecodeError),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            CookbookNetworks(..)
            | Diesel(_)
            | MissingId
            | MissingModel
            | MissingNetworksVersion
            | MissingNodeType
            | NodeCount(_)
            | NodeCountActive(_)
            | NodeCountSyncing(_)
            | NodeCountProvisioning(_)
            | NodeCountFailed(_)
            | UnknownNodeType(_) => Status::internal("Internal error."),
            NodeTypeExists => Status::already_exists("Already exists."),
            ParseId(_) => Status::invalid_argument("id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            Auth(err) => err.into(),
            Blockchain(err) => err.into(),
            BlockchainNodeType(err) => err.into(),
            BlockchainVersion(err) => err.into(),
            Claims(err) => err.into(),
            Command(err) => err.into(),
            CommandGrpc(err) => err.into(),
            Node(err) => err.into(),
            NodeLog(err) => err.into(),
            NodeType(err) => err.into(),
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

    async fn add_node_type(
        &self,
        req: Request<api::BlockchainServiceAddNodeTypeRequest>,
    ) -> Result<Response<api::BlockchainServiceAddNodeTypeResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| add_node_type(req, meta, write).scope_boxed())
            .await
    }

    async fn add_version(
        &self,
        req: Request<api::BlockchainServiceAddVersionRequest>,
    ) -> Result<Response<api::BlockchainServiceAddVersionResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| add_version(req, meta, write).scope_boxed())
            .await
    }
}

async fn get(
    req: api::BlockchainServiceGetRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainServiceGetResponse, Error> {
    let authz = match read.auth_all(&meta, BlockchainAdminPerm::Get).await {
        Ok(authz) => Ok(authz),
        Err(crate::auth::Error::Claims(_)) => read.auth_all(&meta, BlockchainPerm::Get).await,
        Err(err) => Err(err),
    }?;

    let id = req.id.parse().map_err(Error::ParseId)?;
    let blockchain = Blockchain::find_by_id(id, &mut read).await?;
    let mut networks = blockchain_networks([&blockchain], &read.ctx.cookbook, &mut read).await?;

    let node_stats = if let Some(id) = req.org_id {
        let org_id = id.parse().map_err(Error::ParseOrgId)?;
        NodeStats::for_org(org_id, &authz, &mut read).await
    } else {
        NodeStats::for_all(&authz, &mut read).await
    }?;
    let node_stats = node_stats.map(|stats| stats.to_map_keep_last(|ns| (ns.blockchain_id, ns)));

    let blockchain =
        api::Blockchain::from_model(blockchain, &mut networks, node_stats, &mut read).await?;

    Ok(api::BlockchainServiceGetResponse {
        blockchain: Some(blockchain),
    })
}

async fn list(
    req: api::BlockchainServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainServiceListResponse, Error> {
    let authz = match read.auth_all(&meta, BlockchainAdminPerm::List).await {
        Ok(authz) => Ok(authz),
        Err(crate::auth::Error::Claims(_)) => read.auth_all(&meta, BlockchainPerm::List).await,
        Err(err) => Err(err),
    }?;

    let blockchains = Blockchain::find_all(&mut read).await?;
    let blockchain_refs = blockchains.iter().collect::<Vec<_>>();
    let mut networks = blockchain_networks(blockchain_refs, &read.ctx.cookbook, &mut read).await?;

    let node_stats = if let Some(id) = req.org_id {
        let org_id = id.parse().map_err(Error::ParseOrgId)?;
        NodeStats::for_org(org_id, &authz, &mut read).await
    } else {
        NodeStats::for_all(&authz, &mut read).await
    }?;
    let node_stats = node_stats.map(|stats| stats.to_map_keep_last(|ns| (ns.blockchain_id, ns)));

    let blockchains =
        api::Blockchain::from_models(blockchains, &mut networks, node_stats, &mut read).await?;

    Ok(api::BlockchainServiceListResponse { blockchains })
}

/// Add a new `NodeType` to an existing blockchain.
async fn add_node_type(
    req: api::BlockchainServiceAddNodeTypeRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::BlockchainServiceAddNodeTypeResponse, Error> {
    write
        .auth_all(&meta, BlockchainAdminPerm::AddNodeType)
        .await?;

    let id = req.id.parse().map_err(Error::ParseId)?;
    let node_type = api::NodeType::try_from(req.node_type)
        .map(NodeType::from)
        .map_err(Error::UnknownNodeType)?;

    let node_types = BlockchainNodeType::by_blockchain_id(id, &mut write).await?;
    let node_types: HashSet<_> = node_types.iter().map(|nt| nt.node_type).collect();
    if node_types.contains(&node_type) {
        return Err(Error::NodeTypeExists);
    }

    NewBlockchainNodeType::new(id, node_type, req.description)
        .create(&mut write)
        .await?;

    Ok(api::BlockchainServiceAddNodeTypeResponse {})
}

/// Add a new blockchain version for some existing `node_type`.
///
/// This will trigger `UpgradeNode` commands for older nodes of this type.
///
/// The transaction will fail if it can't retrieve cookbook networks from:
/// `{blockchain}/{node_type}/{version}/babel.rhai`
async fn add_version(
    req: api::BlockchainServiceAddVersionRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::BlockchainServiceAddVersionResponse, Error> {
    write
        .auth_all(&meta, BlockchainAdminPerm::AddVersion)
        .await?;

    let id = req.id.parse().map_err(Error::ParseId)?;
    let blockchain = Blockchain::find_by_id(id, &mut write).await?;
    let node_type = api::NodeType::try_from(req.node_type)
        .map(NodeType::from)
        .map_err(Error::UnknownNodeType)?;

    let node_version = NodeVersion::new(&req.version)?;
    let new_version =
        NewVersion::new(id, node_type, &node_version, req.description, &mut write).await?;
    let version = new_version.create(&mut write).await?;

    let image = Image::new(&blockchain.name, node_type, node_version.clone());
    let (_, networks) = cookbook_networks(&write.ctx.cookbook, &image, version.id).await?;

    let properties = req
        .properties
        .iter()
        .map(|property| NewProperty::new(&version, property.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    NewProperty::bulk_create(properties, &mut write).await?;

    let nodes = Node::upgradeable_by_type(id, node_type, &mut write).await?;
    for mut node in nodes {
        let upgrade = upgrade_node(&node, &node_version, &blockchain.name)?;
        if let Some((new_command, new_log)) = upgrade {
            node.version = node_version.clone();
            let node = node.update(&mut write).await?;

            new_log.create(&mut write).await?;
            let command = new_command.create(&mut write).await?;

            write.mqtt(upgrade_command(node.id, &command, image.clone()));
        }
    }

    let version =
        BlockchainVersion::find(blockchain.id, node_type, &node_version, &mut write).await?;
    let properties = BlockchainProperty::by_version_id(version.id, &mut write).await?;

    Ok(api::BlockchainServiceAddVersionResponse {
        version: Some(api::BlockchainVersion::from_model(
            version, networks, properties,
        )),
    })
}

fn upgrade_node<'b, 'n>(
    node: &'n Node,
    node_version: &'n NodeVersion,
    blockchain_name: &'b str,
) -> Result<Option<(NewCommand, NewNodeLog<'b>)>, Error> {
    if node_version.semver()? <= node.version.semver()? {
        return Ok(None);
    }

    let command = NewCommand {
        host_id: node.host_id,
        cmd: CommandType::UpgradeNode,
        node_id: Some(node.id),
    };

    let log = NewNodeLog {
        host_id: node.host_id,
        node_id: node.id,
        event: NodeLogEvent::Upgraded,
        blockchain_name,
        node_type: node.node_type,
        version: node_version.clone(),
        created_at: Utc::now(),
    };

    Ok(Some((command, log)))
}

/// Take our new Command and turn it into a `gRPC` message
fn upgrade_command(node_id: NodeId, command: &Command, image: Image) -> api::Command {
    api::Command {
        id: command.id.to_string(),
        exit_code: None,
        exit_message: None,
        retry_hint_seconds: None,
        acked_at: None,
        command: Some(api::command::Command::Node(api::NodeCommand {
            node_id: node_id.to_string(),
            host_id: command.host_id.to_string(),
            command: Some(api::node_command::Command::Upgrade(api::NodeUpgrade {
                image: Some(api::ContainerImage::from(image)),
            })),
            api_command_id: command.id.to_string(),
            created_at: Some(NanosUtc::from(command.created_at).into()),
        })),
    }
}

/// For each blockchain version, retrieve a list of networks from cookbook.
async fn blockchain_networks<'b, B>(
    blockchains: B,
    cookbook: &Cookbook,
    conn: &mut Conn<'_>,
) -> Result<HashMap<BlockchainVersionId, Vec<api::BlockchainNetwork>>, Error>
where
    B: AsRef<[&'b Blockchain]> + Send,
{
    let chain_ids: HashSet<_> = blockchains.as_ref().iter().map(|b| b.id).collect();
    let chain_map = blockchains.as_ref().iter().to_map_keep_last(|b| (b.id, b));

    let node_types = BlockchainNodeType::by_blockchain_ids(chain_ids.clone(), conn).await?;
    let node_types = node_types.to_map_keep_last(|nt| (nt.id, nt));

    let versions = BlockchainVersion::by_blockchain_ids(chain_ids, conn).await?;
    let version_ids = versions
        .iter()
        .map(|row| {
            let blockchain = chain_map.get(&row.blockchain_id).ok_or(Error::MissingId)?;
            let node_type = node_types
                .get(&row.blockchain_node_type_id)
                .ok_or(Error::MissingNodeType)?
                .node_type;
            let node_version = NodeVersion::new(&row.version)?;
            let image = Image::new(&blockchain.name, node_type, node_version);
            Ok((row.id, image))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let network_futs = version_ids
        .iter()
        .map(|(version_id, image)| cookbook_networks(cookbook, image, *version_id));

    let mut networks: HashMap<_, Vec<_>> = HashMap::new();
    for result in join_all(network_futs).await {
        match result {
            Ok((version_id, nets)) => {
                networks.entry(version_id).or_default().extend(nets);
            }
            Err(err) => warn!("Failed to get cookbook networks: {err}"),
        }
    }

    Ok(networks)
}

/// Retrieve a list of networks from cookbook for some `image`.
async fn cookbook_networks(
    cookbook: &Cookbook,
    image: &Image,
    version_id: BlockchainVersionId,
) -> Result<(BlockchainVersionId, Vec<api::BlockchainNetwork>), Error> {
    let metadata = cookbook
        .rhai_metadata(image)
        .await
        .map_err(|err| Error::CookbookNetworks(image.clone(), err))?;

    let networks = metadata
        .nets
        .into_iter()
        .map(|(name, network)| api::BlockchainNetwork {
            name,
            url: network.url,
            net_type: api::NetType::from(network.net_type) as i32,
        })
        .collect();

    Ok((version_id, networks))
}

impl api::Blockchain {
    async fn from_models(
        models: Vec<Blockchain>,
        networks: &mut HashMap<BlockchainVersionId, Vec<api::BlockchainNetwork>>,
        node_stats: Option<HashMap<BlockchainId, NodeStats>>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let ids: HashSet<_> = models.iter().map(|blockchain| blockchain.id).collect();

        let node_types = BlockchainNodeType::by_blockchain_ids(ids.clone(), conn).await?;
        let mut node_types = node_types.to_map_keep_all(|nt| (nt.blockchain_id, nt));

        let versions = BlockchainVersion::by_blockchain_ids(ids.clone(), conn).await?;
        let mut versions = versions.to_map_keep_all(|v| (v.blockchain_node_type_id, v));

        let properties = BlockchainProperty::by_blockchain_ids(ids, conn).await?;
        let mut properties = properties.to_map_keep_all(|p| (p.blockchain_version_id, p));

        models
            .into_iter()
            .map(|model| {
                let node_types = node_types.remove(&model.id).unwrap_or_default();
                let node_types = api::BlockchainNodeType::from_models(
                    node_types,
                    &mut versions,
                    networks,
                    &mut properties,
                );

                let stats = node_stats
                    .as_ref()
                    .map(|stats| api::BlockchainStats::from_model(&model, stats))
                    .transpose()?;

                Ok(api::Blockchain {
                    id: model.id.to_string(),
                    name: model.name,
                    description: model.description,
                    project_url: model.project_url,
                    repo_url: model.repo_url,
                    node_types,
                    created_at: Some(NanosUtc::from(model.created_at).into()),
                    updated_at: Some(NanosUtc::from(model.updated_at).into()),
                    stats,
                })
            })
            .collect()
    }

    async fn from_model(
        blockchain: Blockchain,
        networks: &mut HashMap<BlockchainVersionId, Vec<api::BlockchainNetwork>>,
        node_stats: Option<HashMap<BlockchainId, NodeStats>>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let mut blockchains = Self::from_models(vec![blockchain], networks, node_stats, conn)
            .await?
            .into_iter();

        match (blockchains.next(), blockchains.next()) {
            (Some(blockchain), None) => Ok(blockchain),
            _ => Err(Error::MissingModel),
        }
    }
}

impl api::BlockchainNodeType {
    fn from_models(
        node_types: Vec<BlockchainNodeType>,
        versions: &mut HashMap<BlockchainNodeTypeId, Vec<BlockchainVersion>>,
        networks: &mut HashMap<BlockchainVersionId, Vec<api::BlockchainNetwork>>,
        properties: &mut HashMap<BlockchainVersionId, Vec<BlockchainProperty>>,
    ) -> Vec<Self> {
        node_types
            .into_iter()
            .map(|node_type| {
                let versions = versions.remove(&node_type.id).unwrap_or_default();
                Self::from_model(node_type, versions, networks, properties)
            })
            .collect()
    }

    fn from_model(
        node_type: BlockchainNodeType,
        versions: Vec<BlockchainVersion>,
        networks: &mut HashMap<BlockchainVersionId, Vec<api::BlockchainNetwork>>,
        properties: &mut HashMap<BlockchainVersionId, Vec<BlockchainProperty>>,
    ) -> Self {
        api::BlockchainNodeType {
            id: node_type.id.to_string(),
            node_type: api::NodeType::from(node_type.node_type).into(),
            versions: api::BlockchainVersion::from_models(versions, networks, properties),
            description: node_type.description,
            created_at: Some(NanosUtc::from(node_type.created_at).into()),
            updated_at: Some(NanosUtc::from(node_type.updated_at).into()),
        }
    }
}

impl api::BlockchainVersion {
    fn from_models(
        models: Vec<BlockchainVersion>,
        networks: &mut HashMap<BlockchainVersionId, Vec<api::BlockchainNetwork>>,
        properties: &mut HashMap<BlockchainVersionId, Vec<BlockchainProperty>>,
    ) -> Vec<Self> {
        models
            .into_iter()
            .map(|model| {
                let networks = networks.remove(&model.id).unwrap_or_else(|| {
                    warn!("No networks for blockchain version `{}`", model.id);
                    vec![]
                });
                let properties = properties.remove(&model.id).unwrap_or_default();

                Self::from_model(model, networks, properties)
            })
            .collect()
    }

    fn from_model(
        version: BlockchainVersion,
        networks: Vec<api::BlockchainNetwork>,
        properties: Vec<BlockchainProperty>,
    ) -> Self {
        api::BlockchainVersion {
            id: version.id.to_string(),
            version: version.version,
            description: version.description,
            created_at: Some(NanosUtc::from(version.created_at).into()),
            updated_at: Some(NanosUtc::from(version.updated_at).into()),
            networks,
            properties: properties.into_iter().map(Into::into).collect(),
        }
    }
}

impl api::BlockchainStats {
    fn from_model(
        model: &Blockchain,
        node_stats: &HashMap<BlockchainId, NodeStats>,
    ) -> Result<Self, Error> {
        let stats = node_stats.get(&model.id);

        let count = stats.map(|s| s.node_count).unwrap_or_default();
        let active = stats.map(|s| s.node_count_active).unwrap_or_default();
        let syncing = stats.map(|s| s.node_count_syncing).unwrap_or_default();
        let provisioning = stats.map(|s| s.node_count_provisioning).unwrap_or_default();
        let failed = stats.map(|s| s.node_count_failed).unwrap_or_default();

        Ok(api::BlockchainStats {
            node_count: Some(count.try_into().map_err(Error::NodeCount)?),
            node_count_active: Some(active.try_into().map_err(Error::NodeCountActive)?),
            node_count_syncing: Some(syncing.try_into().map_err(Error::NodeCountSyncing)?),
            node_count_provisioning: Some(
                provisioning
                    .try_into()
                    .map_err(Error::NodeCountProvisioning)?,
            ),
            node_count_failed: Some(failed.try_into().map_err(Error::NodeCountFailed)?),
        })
    }
}
