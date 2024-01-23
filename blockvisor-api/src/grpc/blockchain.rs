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
use crate::auth::{AuthZ, Authorize};
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::models::blockchain::{
    Blockchain, BlockchainId, BlockchainNodeType, BlockchainNodeTypeId, BlockchainProperty,
    BlockchainVersion, BlockchainVersionId, NewBlockchainNodeType, NewProperty, NewVersion,
    NodeStats,
};
use crate::models::command::NewCommand;
use crate::models::node::{NewNodeLog, Node, NodeLogEvent, NodeType, NodeVersion, UpdateNode};
use crate::models::{Command, CommandType};
use crate::storage::image::ImageId;
use crate::storage::Storage;
use crate::util::{HashVec, NanosUtc};

use super::api::blockchain_service_server::BlockchainService;
use super::{api, common, Grpc};

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
    /// Blockchain command failed: {0}
    Command(#[from] crate::models::command::Error),
    /// Blockchain command failed: {0}
    CommandGrpc(#[from] crate::grpc::command::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing image identifier.
    MissingImageId,
    /// Missing `api::Blockchain` model output. This should not happen.
    MissingModel,
    /// Missing BlockchainVersionId in networks. This should not happen.
    MissingNetworksVersion,
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
    /// Failed to parse ImageId: {0}
    ParseImageId(crate::storage::image::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Blockchain property error: {0}
    Property(#[from] crate::models::blockchain::property::Error),
    /// Storage failed: {0}
    Storage(#[from] crate::storage::Error),
    /// Blockchain failed to get storage networks for `{0:?}`: {1}
    StorageNetworks(ImageId, crate::storage::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_)
            | MissingModel
            | MissingNetworksVersion
            | NodeCount(_)
            | NodeCountActive(_)
            | NodeCountSyncing(_)
            | NodeCountProvisioning(_)
            | NodeCountFailed(_)
            | Storage(_)
            | StorageNetworks(..) => Status::internal("Internal error."),
            NodeTypeExists => Status::already_exists("Already exists."),
            MissingImageId | ParseImageId(_) => Status::invalid_argument("id"),
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

    async fn get_image(
        &self,
        req: Request<api::BlockchainServiceGetImageRequest>,
    ) -> Result<Response<api::BlockchainServiceGetImageResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_image(req, meta, read).scope_boxed())
            .await
    }

    async fn get_plugin(
        &self,
        req: Request<api::BlockchainServiceGetPluginRequest>,
    ) -> Result<Response<api::BlockchainServiceGetPluginResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_plugin(req, meta, read).scope_boxed())
            .await
    }

    async fn get_requirements(
        &self,
        req: Request<api::BlockchainServiceGetRequirementsRequest>,
    ) -> Result<Response<api::BlockchainServiceGetRequirementsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_requirements(req, meta, read).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::BlockchainServiceListRequest>,
    ) -> Result<Response<api::BlockchainServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }

    async fn list_image_versions(
        &self,
        req: Request<api::BlockchainServiceListImageVersionsRequest>,
    ) -> Result<Response<api::BlockchainServiceListImageVersionsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_image_versions(req, meta, read).scope_boxed())
            .await
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
    let blockchain = Blockchain::by_id(id, &authz, &mut read).await?;
    let mut networks =
        blockchain_networks([&blockchain], &read.ctx.storage, &authz, &mut read).await?;

    let node_stats = if let Some(id) = req.org_id {
        let org_id = id.parse().map_err(Error::ParseOrgId)?;
        NodeStats::for_org(org_id, &authz, &mut read).await
    } else {
        NodeStats::for_all(&authz, &mut read).await
    }?;
    let node_stats = node_stats.map(|stats| stats.to_map_keep_last(|ns| (ns.blockchain_id, ns)));

    let blockchain =
        api::Blockchain::from_model(blockchain, &mut networks, node_stats, &authz, &mut read)
            .await?;

    Ok(api::BlockchainServiceGetResponse {
        blockchain: Some(blockchain),
    })
}

async fn get_image(
    req: api::BlockchainServiceGetImageRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainServiceGetImageResponse, Error> {
    read.auth_all(&meta, BlockchainPerm::GetImage).await?;

    let id = req.id.ok_or(Error::MissingImageId)?;
    let image = ImageId::try_from(id).map_err(Error::ParseImageId)?;
    let url = read.ctx.storage.download_image(&image).await?;

    Ok(api::BlockchainServiceGetImageResponse {
        location: Some(common::ArchiveLocation {
            url: url.to_string(),
        }),
    })
}

async fn get_plugin(
    req: api::BlockchainServiceGetPluginRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainServiceGetPluginResponse, Error> {
    read.auth_all(&meta, BlockchainPerm::GetPlugin).await?;

    let id = req.id.ok_or(Error::MissingImageId)?;
    let image = ImageId::try_from(id).map_err(Error::ParseImageId)?;
    let rhai_content = read.ctx.storage.rhai_script(&image).await?;

    let plugin = common::RhaiPlugin {
        identifier: Some(image.into()),
        rhai_content,
    };

    Ok(api::BlockchainServiceGetPluginResponse {
        plugin: Some(plugin),
    })
}

async fn get_requirements(
    req: api::BlockchainServiceGetRequirementsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainServiceGetRequirementsResponse, Error> {
    read.auth_all(&meta, BlockchainPerm::GetRequirements)
        .await?;

    let id = req.id.ok_or(Error::MissingImageId)?;
    let image = ImageId::try_from(id).map_err(Error::ParseImageId)?;
    let requirements = read.ctx.storage.rhai_metadata(&image).await?.requirements;

    Ok(api::BlockchainServiceGetRequirementsResponse {
        vcpu_count: requirements.vcpu_count,
        mem_size_bytes: requirements.mem_size_mb * 1000 * 1000,
        disk_size_bytes: requirements.disk_size_gb * 1000 * 1000 * 1000,
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

    let blockchains = Blockchain::find_all(&authz, &mut read).await?;
    let blockchain_refs = blockchains.iter().collect::<Vec<_>>();
    let mut networks =
        blockchain_networks(blockchain_refs, &read.ctx.storage, &authz, &mut read).await?;

    let node_stats = if let Some(id) = req.org_id {
        let org_id = id.parse().map_err(Error::ParseOrgId)?;
        NodeStats::for_org(org_id, &authz, &mut read).await
    } else {
        NodeStats::for_all(&authz, &mut read).await
    }?;
    let node_stats = node_stats.map(|stats| stats.to_map_keep_last(|ns| (ns.blockchain_id, ns)));

    let blockchains =
        api::Blockchain::from_models(blockchains, &mut networks, node_stats, &authz, &mut read)
            .await?;

    Ok(api::BlockchainServiceListResponse { blockchains })
}

async fn list_image_versions(
    req: api::BlockchainServiceListImageVersionsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BlockchainServiceListImageVersionsResponse, Error> {
    let _ = read
        .auth_all(&meta, BlockchainPerm::ListImageVersions)
        .await?;

    let node_type = req.node_type().into();
    let idents = read.ctx.storage.list(&req.protocol, node_type).await?;
    let identifiers = idents.into_iter().map(Into::into).collect();

    Ok(api::BlockchainServiceListImageVersionsResponse { identifiers })
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
    let node_type = NodeType::from(req.node_type());
    if BlockchainNodeType::exists(id, node_type, &mut write).await? {
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
/// The transaction will fail if it can't retrieve storage networks from:
/// `{blockchain}/{node_type}/{version}/babel.rhai`
async fn add_version(
    req: api::BlockchainServiceAddVersionRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::BlockchainServiceAddVersionResponse, Error> {
    let authz = write
        .auth_all(&meta, BlockchainAdminPerm::AddVersion)
        .await?;

    let id = req.id.parse().map_err(Error::ParseId)?;
    let blockchain = Blockchain::by_id(id, &authz, &mut write).await?;
    let node_type = NodeType::from(req.node_type());
    let node_version = NodeVersion::new(&req.version)?;
    let version = NewVersion::new(
        id,
        node_type,
        &node_version,
        req.description,
        &authz,
        &mut write,
    )
    .await?
    .create(&mut write)
    .await?;

    let image = ImageId::new(&blockchain.name, node_type, node_version.clone());
    let (_, networks) = storage_networks(&write.ctx.storage, &image, version.id).await?;

    let properties = req
        .properties
        .iter()
        .map(|property| NewProperty::new(&version, property.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    NewProperty::bulk_create(properties, &mut write).await?;

    let nodes = Node::upgradeable_by_type(id, node_type, &mut write).await?;
    for node in nodes {
        let upgrade = upgrade_node(&node, &node_version, &blockchain)?;
        if let Some((new_command, new_log)) = upgrade {
            let update = UpdateNode {
                version: Some(&node_version),
                ..Default::default()
            };
            let node = node.update(update, &mut write).await?;

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

fn upgrade_node(
    node: &Node,
    node_version: &NodeVersion,
    blockchain: &Blockchain,
) -> Result<Option<(NewCommand, NewNodeLog)>, Error> {
    if node_version.semver()? <= node.version.semver()? {
        return Ok(None);
    }

    let command = NewCommand::node(node, CommandType::NodeUpgrade)?;

    let log = NewNodeLog {
        host_id: node.host_id,
        node_id: node.id,
        event: NodeLogEvent::Upgraded,
        blockchain_id: blockchain.id,
        node_type: node.node_type,
        version: node_version.clone(),
        created_at: Utc::now(),
        org_id: node.org_id,
    };

    Ok(Some((command, log)))
}

/// Take our new Command and turn it into a `gRPC` message
fn upgrade_command(node_id: NodeId, command: &Command, image: ImageId) -> api::Command {
    api::Command {
        id: command.id.to_string(),
        exit_code: None,
        exit_message: None,
        retry_hint_seconds: None,
        created_at: Some(NanosUtc::from(command.created_at).into()),
        acked_at: None,
        command: Some(api::command::Command::Node(api::NodeCommand {
            node_id: node_id.to_string(),
            host_id: command.host_id.to_string(),
            command: Some(api::node_command::Command::Upgrade(api::NodeUpgrade {
                image: Some(image.into()),
            })),
        })),
    }
}

/// For each blockchain version, retrieve a list of networks from storage.
async fn blockchain_networks<'b, B>(
    blockchains: B,
    storage: &Storage,
    authz: &AuthZ,
    conn: &mut Conn<'_>,
) -> Result<HashMap<BlockchainVersionId, Vec<common::NetworkConfig>>, Error>
where
    B: AsRef<[&'b Blockchain]> + Send,
{
    let chain_ids: HashSet<_> = blockchains.as_ref().iter().map(|b| b.id).collect();
    let chain_map = blockchains.as_ref().iter().to_map_keep_last(|b| (b.id, b));

    let node_types = BlockchainNodeType::by_blockchain_ids(chain_ids.clone(), authz, conn).await?;
    let node_types = node_types.to_map_keep_last(|nt| (nt.id, nt));

    let versions = BlockchainVersion::by_blockchain_ids(chain_ids, conn).await?;
    let version_ids = versions
        .iter()
        .filter_map(|row| {
            let blockchain = chain_map.get(&row.blockchain_id)?;
            let node_type = node_types.get(&row.blockchain_node_type_id)?.node_type;
            let image_result = NodeVersion::new(&row.version)
                .map_err(Into::into)
                .map(|version| ImageId::new(&blockchain.name, node_type, version))
                .map(|image| (row.id, image));
            Some(image_result)
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let network_futs = version_ids
        .iter()
        .map(|(version_id, image)| storage_networks(storage, image, *version_id));

    let mut networks: HashMap<_, Vec<_>> = HashMap::new();
    for result in join_all(network_futs).await {
        match result {
            Ok((version_id, nets)) => {
                networks.entry(version_id).or_default().extend(nets);
            }
            Err(err) => warn!("Failed to get storage networks: {err}"),
        }
    }

    Ok(networks)
}

/// Retrieve a list of networks from storage for some `image`.
async fn storage_networks(
    storage: &Storage,
    image: &ImageId,
    version_id: BlockchainVersionId,
) -> Result<(BlockchainVersionId, Vec<common::NetworkConfig>), Error> {
    let metadata = storage
        .rhai_metadata(image)
        .await
        .map_err(|err| Error::StorageNetworks(image.clone(), err))?;

    let networks = metadata
        .networks
        .into_iter()
        .map(|(name, network)| common::NetworkConfig {
            name,
            url: network.url.to_string(),
            net_type: common::NetType::from(network.net_type).into(),
            metadata: hashmap! {},
        })
        .collect();

    Ok((version_id, networks))
}

impl api::Blockchain {
    async fn from_models(
        models: Vec<Blockchain>,
        networks: &mut HashMap<BlockchainVersionId, Vec<common::NetworkConfig>>,
        node_stats: Option<HashMap<BlockchainId, NodeStats>>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let ids: HashSet<_> = models.iter().map(|blockchain| blockchain.id).collect();

        let node_types = BlockchainNodeType::by_blockchain_ids(ids.clone(), authz, conn).await?;
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
                    visibility: api::BlockchainVisibility::from(model.visibility).into(),
                    ticker: model.ticker,
                })
            })
            .collect()
    }

    async fn from_model(
        blockchain: Blockchain,
        networks: &mut HashMap<BlockchainVersionId, Vec<common::NetworkConfig>>,
        node_stats: Option<HashMap<BlockchainId, NodeStats>>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let mut blockchains =
            Self::from_models(vec![blockchain], networks, node_stats, authz, conn)
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
        networks: &mut HashMap<BlockchainVersionId, Vec<common::NetworkConfig>>,
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
        networks: &mut HashMap<BlockchainVersionId, Vec<common::NetworkConfig>>,
        properties: &mut HashMap<BlockchainVersionId, Vec<BlockchainProperty>>,
    ) -> Self {
        api::BlockchainNodeType {
            id: node_type.id.to_string(),
            node_type: common::NodeType::from(node_type.node_type).into(),
            versions: api::BlockchainVersion::from_models(versions, networks, properties),
            description: node_type.description,
            created_at: Some(NanosUtc::from(node_type.created_at).into()),
            updated_at: Some(NanosUtc::from(node_type.updated_at).into()),
            visibility: api::BlockchainVisibility::from(node_type.visibility).into(),
        }
    }
}

impl api::BlockchainVersion {
    fn from_models(
        models: Vec<BlockchainVersion>,
        networks: &mut HashMap<BlockchainVersionId, Vec<common::NetworkConfig>>,
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
        networks: Vec<common::NetworkConfig>,
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
