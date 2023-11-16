use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use futures_util::future::OptionFuture;
use petname::{Generator, Petnames};
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;
use uuid::Uuid;

use crate::auth::rbac::{NodeAdminPerm, NodePerm};
use crate::auth::resource::{HostId, NodeId, Resource, ResourceEntry, ResourceType};
use crate::auth::Authorize;
use crate::cookbook::image::Image;
use crate::cookbook::script::HardwareRequirements;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::models::blockchain::{BlockchainProperty, BlockchainPropertyId, BlockchainVersion};
use crate::models::command::NewCommand;
use crate::models::node::{
    ContainerStatus, FilteredIpAddr, NewNode, Node, NodeFilter, NodeJob, NodeJobProgress,
    NodeJobStatus, NodeProperty, NodeScheduler, NodeSearch, NodeStakingStatus, NodeStatus,
    NodeSyncStatus, NodeType, UpdateNode,
};
use crate::models::{Blockchain, Command, CommandType, Host, Org, Region, User};
use crate::util::{HashVec, NanosUtc};

use super::api::node_service_server::NodeService;
use super::{api, common, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse allow ips: {0}
    AllowIps(serde_json::Error),
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Node blockchain error: {0}
    Blockchain(#[from] crate::models::blockchain::Error),
    /// Node blockchain property error: {0}
    BlockchainProperty(#[from] crate::models::blockchain::property::Error),
    /// Node blockchain property error: {0}
    BlockchainVersion(#[from] crate::models::blockchain::version::Error),
    /// Failed to parse block height: {0}
    BlockHeight(std::num::TryFromIntError),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Node command error: {0}
    Command(#[from] crate::models::command::Error),
    /// Node grpc command error: {0}
    CommandGrpc(#[from] crate::grpc::command::Error),
    /// Node cookbook error: {0}
    Cookbook(#[from] crate::cookbook::Error),
    /// Failed to parse deny ips: {0}
    DenyIps(serde_json::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Failed to parse disk size bytes: {0}
    DiskSize(std::num::TryFromIntError),
    /// Failed to generate petnames. This should not happen.
    GeneratePetnames,
    /// Node host error: {0}
    Host(#[from] crate::models::host::Error),
    /// Node ip address error: {0}
    IpAddress(#[from] crate::models::ip_address::Error),
    /// Failed to parse mem size bytes: {0}
    MemSize(std::num::TryFromIntError),
    /// Missing placement.
    MissingPlacement,
    /// Missing blockchain property id: {0}.
    MissingPropertyId(BlockchainPropertyId),
    /// Node model error: {0}
    Model(#[from] crate::models::node::Error),
    /// Node model property error: {0}
    ModelProperty(#[from] crate::models::node::property::Error),
    /// No ResourceAffinity.
    NoResourceAffinity,
    /// The NodeStatus that was received was `Unspecified`
    NodeStatusUnspecified,
    /// Node org error: {0}
    Org(#[from] crate::models::org::Error),
    /// Failed to parse BlockchainId: {0}
    ParseBlockchainId(uuid::Error),
    /// Failed to parse HostId: {0}
    ParseHostId(uuid::Error),
    /// Failed to parse NodeId: {0}
    ParseId(uuid::Error),
    /// Failed to parse IpAddr: {0}
    ParseIpAddr(std::net::AddrParseError),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Blockchain property not found: {0}
    PropertyNotFound(String),
    /// Node region error: {0}
    Region(#[from] crate::models::region::Error),
    /// Node resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// Node search failed: {0}
    SearchOperator(crate::util::search::Error),
    /// Failed to parse current data sync progress: {0}
    SyncCurrent(std::num::TryFromIntError),
    /// Failed to parse total data sync progress: {0}
    SyncTotal(std::num::TryFromIntError),
    /// Node user error: {0}
    User(#[from] crate::models::user::Error),
    /// Failed to parse virtual cpu count: {0}
    Vcpu(std::num::TryFromIntError),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Cookbook(_) | Diesel(_) | GeneratePetnames | MissingPropertyId(_)
            | ModelProperty(_) | ParseIpAddr(_) | PropertyNotFound(_) => {
                Status::internal("Internal error.")
            }
            AllowIps(_) => Status::invalid_argument("allow_ips"),
            BlockHeight(_) => Status::invalid_argument("block_height"),
            DenyIps(_) => Status::invalid_argument("deny_ips"),
            DiskSize(_) => Status::invalid_argument("disk_size_bytes"),
            MemSize(_) => Status::invalid_argument("mem_size_bytes"),
            MissingPlacement => Status::invalid_argument("placement"),
            NoResourceAffinity => Status::invalid_argument("resource"),
            NodeStatusUnspecified => Status::invalid_argument("node_status"),
            ParseBlockchainId(_) => Status::invalid_argument("blockchain_id"),
            ParseHostId(_) => Status::invalid_argument("host_id"),
            ParseId(_) => Status::invalid_argument("id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SyncCurrent(_) => Status::invalid_argument("data_sync_progress_current"),
            SyncTotal(_) => Status::invalid_argument("data_sync_progress_total"),
            Vcpu(_) => Status::invalid_argument("vcpu_count"),
            Auth(err) => err.into(),
            Blockchain(err) => err.into(),
            BlockchainProperty(err) => err.into(),
            BlockchainVersion(err) => err.into(),
            Claims(err) => err.into(),
            Command(err) => err.into(),
            CommandGrpc(err) => err.into(),
            Host(err) => err.into(),
            IpAddress(err) => err.into(),
            Model(err) => err.into(),
            Org(err) => err.into(),
            Region(err) => err.into(),
            Resource(err) => err.into(),
            User(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl NodeService for Grpc {
    async fn create(
        &self,
        req: Request<api::NodeServiceCreateRequest>,
    ) -> Result<Response<api::NodeServiceCreateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta, write).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: Request<api::NodeServiceGetRequest>,
    ) -> Result<Response<api::NodeServiceGetResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta, read).scope_boxed()).await
    }

    async fn list(
        &self,
        req: Request<api::NodeServiceListRequest>,
    ) -> Result<Response<api::NodeServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }

    async fn update_config(
        &self,
        req: Request<api::NodeServiceUpdateConfigRequest>,
    ) -> Result<Response<api::NodeServiceUpdateConfigResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_config(req, meta, write).scope_boxed())
            .await
    }

    async fn update_status(
        &self,
        req: Request<api::NodeServiceUpdateStatusRequest>,
    ) -> Result<Response<api::NodeServiceUpdateStatusResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_status(req, meta, write).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::NodeServiceDeleteRequest>,
    ) -> Result<Response<api::NodeServiceDeleteResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta, write).scope_boxed())
            .await
    }

    async fn start(
        &self,
        req: Request<api::NodeServiceStartRequest>,
    ) -> Result<Response<api::NodeServiceStartResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| start(req, meta, write).scope_boxed())
            .await
    }

    async fn stop(
        &self,
        req: Request<api::NodeServiceStopRequest>,
    ) -> Result<Response<api::NodeServiceStopResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| stop(req, meta, write).scope_boxed())
            .await
    }

    async fn restart(
        &self,
        req: Request<api::NodeServiceRestartRequest>,
    ) -> Result<Response<api::NodeServiceRestartResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| restart(req, meta, write).scope_boxed())
            .await
    }
}

async fn get(
    req: api::NodeServiceGetRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::NodeServiceGetResponse, Error> {
    let node_id = req.id.parse().map_err(Error::ParseId)?;
    let node = Node::find_by_id(node_id, &mut read).await?;

    read.auth_or_all(&meta, NodeAdminPerm::Get, NodePerm::Get, node_id)
        .await?;

    Ok(api::NodeServiceGetResponse {
        node: Some(api::Node::from_model(node, &mut read).await?),
    })
}

async fn list(
    req: api::NodeServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::NodeServiceListResponse, Error> {
    let filter = req.as_filter()?;
    if let Some(org_id) = filter.org_id {
        read.auth_or_all(&meta, NodeAdminPerm::List, NodePerm::List, org_id)
            .await?
    } else {
        read.auth_all(&meta, NodeAdminPerm::List).await?
    };

    let (node_count, nodes) = Node::filter(filter, &mut read).await?;
    let nodes = api::Node::from_models(nodes, &mut read).await?;

    Ok(api::NodeServiceListResponse { nodes, node_count })
}

async fn create(
    req: api::NodeServiceCreateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceCreateResponse, Error> {
    // The host_id is either determined by the scheduler, or an optional host_id.
    let (host, authz) = if let Some(host_id) = req.host_id()? {
        let host = Host::find_by_id(host_id, &mut write).await?;
        let authz = write
            .auth_or_all(&meta, NodeAdminPerm::Create, NodePerm::Create, host_id)
            .await?;
        (Some(host), authz)
    } else {
        let authz = write.auth_all(&meta, NodePerm::Create).await?;
        (None, authz)
    };

    let blockchain_id = req
        .blockchain_id
        .parse()
        .map_err(Error::ParseBlockchainId)?;
    let blockchain = Blockchain::find_by_id(blockchain_id, &mut write).await?;

    let node_type = req.node_type().into();
    let image = Image::new(&blockchain.name, node_type, req.version.clone().into());
    let version =
        BlockchainVersion::find(blockchain_id, node_type, &image.node_version, &mut write).await?;

    let requirements = write.ctx.cookbook.rhai_metadata(&image).await?.requirements;
    let created_by = authz.resource();
    let new_node = req.as_new(requirements, created_by, &mut write).await?;
    let node = new_node.create(host, &mut write).await?;

    // The user sends in the properties in a key-value style, that is,
    // { property name: property value }. We want to store this as
    // { property id: property value }. In order to map property names to property ids we can use
    // the id to name map, and then flip the keys and values to create an id to name map. Note that
    // this requires the names to be unique, but we expect this to be the case.
    let name_to_id_map = BlockchainProperty::id_to_name_map(version.id, &mut write)
        .await?
        .into_iter()
        .map(|(k, v)| (v, k))
        .collect();
    let properties = req.properties(&node, &name_to_id_map)?;
    NodeProperty::bulk_create(properties, &mut write).await?;

    let create_notif = create_node_command(&node, CommandType::CreateNode, &mut write).await?;
    let create_cmd = api::Command::from_model(&create_notif, &mut write).await?;
    let start_notif = create_node_command(&node, CommandType::RestartNode, &mut write).await?;
    let start_cmd = api::Command::from_model(&start_notif, &mut write).await?;
    let node_api = api::Node::from_model(node, &mut write).await?;

    let created_by = common::EntityUpdate::from_resource(created_by, &mut write).await?;
    let created = api::NodeMessage::created(node_api.clone(), created_by);

    write.mqtt(create_cmd);
    write.mqtt(created);
    write.mqtt(start_cmd);

    Ok(api::NodeServiceCreateResponse {
        node: Some(node_api),
    })
}

async fn update_config(
    req: api::NodeServiceUpdateConfigRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceUpdateConfigResponse, Error> {
    let node_id: NodeId = req.id.parse().map_err(Error::ParseId)?;
    Node::find_by_id(node_id, &mut write).await?;

    let authz = write
        .auth_or_all(
            &meta,
            NodeAdminPerm::UpdateConfig,
            NodePerm::UpdateConfig,
            node_id,
        )
        .await?;

    let node = req.as_update()?.update(&mut write).await?;
    let updated = create_node_command(&node, CommandType::UpdateNode, &mut write).await?;
    let cmd = api::Command::from_model(&updated, &mut write).await?;

    let node = api::Node::from_model(node, &mut write).await?;
    let updated_by = common::EntityUpdate::from_resource(&authz, &mut write).await?;
    let updated = api::NodeMessage::updated(node, updated_by);

    write.mqtt(cmd);
    write.mqtt(updated);

    Ok(api::NodeServiceUpdateConfigResponse {})
}

async fn update_status(
    req: api::NodeServiceUpdateStatusRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceUpdateStatusResponse, Error> {
    let node_id: NodeId = req.id.parse().map_err(Error::ParseId)?;
    Node::find_by_id(node_id, &mut write).await?;

    let authz = write
        .auth_or_all(
            &meta,
            NodeAdminPerm::UpdateStatus,
            NodePerm::UpdateStatus,
            node_id,
        )
        .await?;

    let node = req.as_update()?.update(&mut write).await?;
    let node = api::Node::from_model(node, &mut write).await?;

    let updated_by = common::EntityUpdate::from_resource(&authz, &mut write).await?;
    let updated = api::NodeMessage::updated(node, updated_by);
    write.mqtt(updated);

    Ok(api::NodeServiceUpdateStatusResponse {})
}

async fn delete(
    req: api::NodeServiceDeleteRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceDeleteResponse, Error> {
    let node_id: NodeId = req.id.parse().map_err(Error::ParseId)?;
    let mut node = Node::find_by_id(node_id, &mut write).await?;

    let authz = write
        .auth_or_all(&meta, NodeAdminPerm::Delete, NodePerm::Delete, node_id)
        .await?;

    node.node_status = NodeStatus::DeletePending;
    let node = node.update(&mut write).await?;
    Node::delete(node.id, &mut write).await?;

    // Send delete node command
    let new_command = NewCommand {
        host_id: node.host_id,
        cmd: CommandType::DeleteNode,
        node_id: Some(node.id),
    };
    let cmd = new_command.create(&mut write).await?;
    let cmd = api::Command::from_model(&cmd, &mut write).await?;

    let deleted_by = common::EntityUpdate::from_resource(&authz, &mut write).await?;
    let deleted = api::NodeMessage::deleted(&node, Some(deleted_by));

    write.mqtt(cmd);
    write.mqtt(deleted);

    Ok(api::NodeServiceDeleteResponse {})
}

async fn start(
    req: api::NodeServiceStartRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceStartResponse, Error> {
    let node_id: NodeId = req.id.parse().map_err(Error::ParseId)?;
    let node = Node::find_by_id(node_id, &mut write).await?;

    write
        .auth_or_all(&meta, NodeAdminPerm::Start, NodePerm::Start, node_id)
        .await?;

    let cmd = create_node_command(&node, CommandType::RestartNode, &mut write).await?;
    let cmd = api::Command::from_model(&cmd, &mut write).await?;

    write.mqtt(cmd);

    Ok(api::NodeServiceStartResponse {})
}

async fn stop(
    req: api::NodeServiceStopRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceStopResponse, Error> {
    let node_id = req.id.parse().map_err(Error::ParseId)?;
    let node = Node::find_by_id(node_id, &mut write).await?;

    write
        .auth_or_all(&meta, NodeAdminPerm::Stop, NodePerm::Stop, node_id)
        .await?;

    let cmd = create_node_command(&node, CommandType::KillNode, &mut write).await?;
    let cmd = api::Command::from_model(&cmd, &mut write).await?;

    write.mqtt(cmd);

    Ok(api::NodeServiceStopResponse {})
}

async fn restart(
    req: api::NodeServiceRestartRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceRestartResponse, Error> {
    let node_id = req.id.parse().map_err(Error::ParseId)?;
    let node = Node::find_by_id(node_id, &mut write).await?;

    write
        .auth_or_all(&meta, NodeAdminPerm::Restart, NodePerm::Restart, node_id)
        .await?;

    let cmd = create_node_command(&node, CommandType::RestartNode, &mut write).await?;
    let cmd = api::Command::from_model(&cmd, &mut write).await?;

    write.mqtt(cmd);

    Ok(api::NodeServiceRestartResponse {})
}

pub(super) async fn create_node_command(
    node: &Node,
    cmd_type: CommandType,
    conn: &mut Conn<'_>,
) -> Result<Command, Error> {
    let new_command = NewCommand {
        host_id: node.host_id,
        cmd: cmd_type,
        node_id: Some(node.id),
    };
    new_command.create(conn).await.map_err(Into::into)
}

impl api::Node {
    /// This function is used to create a ui node from a database node. We want to include the
    /// `database_name` in the ui representation, but it is not in the node model. Therefore we
    /// perform a seperate query to the blockchains table.
    pub async fn from_model(node: Node, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let blockchain = Blockchain::find_by_id(node.blockchain_id, conn).await?;

        // We need to get both the node properties and the blockchain properties to construct the
        // final dto. First we query both, and then we zip them together.
        let node_props = NodeProperty::by_node_id(node.id, conn).await?;
        let property_ids = node_props
            .iter()
            .map(|np| np.blockchain_property_id)
            .collect();
        let block_props = BlockchainProperty::by_property_ids(property_ids, conn)
            .await?
            .to_map_keep_last(|prop| (prop.id, prop));
        let properties = node_props
            .into_iter()
            .map(|node_prop| {
                let id = node_prop.blockchain_property_id;
                let block_prop = block_props.get(&id).ok_or(Error::MissingPropertyId(id))?;
                Ok::<_, Error>(api::NodeProperty::from_model(node_prop, block_prop.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let host = Host::find_by_id(node.host_id, conn).await?;
        let org = Org::find_by_id(node.org_id, conn).await?;
        let region = node.region(conn).await?;
        let created_by = common::EntityUpdate::from_node(&node, conn).await?;

        api::Node::new(
            node,
            &org,
            &host,
            &blockchain,
            properties,
            region.as_ref(),
            created_by,
        )
    }

    /// This function is used to create many ui nodes from many database nodes. The same
    /// justification as above applies. Note that this function does not simply defer to the
    /// function above, but rather it performs 1 query for n nodes. We like it this way :)
    pub async fn from_models(nodes: Vec<Node>, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        let node_ids = nodes.iter().map(|n| n.id).collect();
        let node_props = NodeProperty::by_node_ids(node_ids, conn).await?;
        let property_ids = node_props
            .iter()
            .map(|np| np.blockchain_property_id)
            .collect();

        let blockchain_ids = nodes.iter().map(|n| n.blockchain_id).collect();
        let blockchains = Blockchain::find_by_ids(blockchain_ids, conn)
            .await?
            .to_map_keep_last(|b| (b.id, b));

        let block_props = BlockchainProperty::by_property_ids(property_ids, conn)
            .await?
            .to_map_keep_last(|prop| (prop.id, prop));
        let mut properties = node_props.to_map_keep_all(|node_prop| {
            let node_id = node_prop.node_id;
            let prop_id = node_prop.blockchain_property_id;
            let property = api::NodeProperty::from_model(node_prop, block_props[&prop_id].clone());
            (node_id, property)
        });

        let org_ids = nodes.iter().map(|n| n.org_id).collect();
        let orgs = Org::find_by_ids(org_ids, conn)
            .await?
            .to_map_keep_last(|org| (org.id, org));

        let host_ids = nodes.iter().map(|n| n.host_id).collect();
        let hosts = Host::find_by_ids(host_ids, conn)
            .await?
            .to_map_keep_last(|host| (host.id, host));

        let region_ids = nodes.iter().filter_map(|n| n.scheduler_region).collect();
        let regions = Region::by_ids(region_ids, conn)
            .await?
            .to_map_keep_last(|region| (region.id, region));

        let user_ids = nodes
            .iter()
            .filter_map(|node| match node.created_by_resource {
                Some(ResourceType::User) => node.created_by.map(|id| (*id).into()),
                _ => None,
            })
            .collect();
        let users = User::find_by_ids(user_ids, conn)
            .await?
            .to_map_keep_last(|user| (user.id, user));

        nodes
            .into_iter()
            .filter_map(|node| {
                let org = orgs.get(&node.org_id)?;
                let host = hosts.get(&node.host_id)?;
                let blockchain = blockchains.get(&node.blockchain_id)?;
                let properties = properties.remove(&node.id).unwrap_or_default();
                let region = node.scheduler_region.map(|id| &regions[&id]);
                let user = node.created_by.and_then(|id| users.get(&(*id).into()));
                let created_by = common::EntityUpdate::from_node_user(&node, user);

                Some(api::Node::new(
                    node, org, host, blockchain, properties, region, created_by,
                ))
            })
            .collect()
    }

    pub fn new(
        node: Node,
        org: &Org,
        host: &Host,
        blockchain: &Blockchain,
        properties: Vec<api::NodeProperty>,
        region: Option<&Region>,
        created_by: Option<common::EntityUpdate>,
    ) -> Result<Self, Error> {
        let scheduler = node
            .scheduler_resource
            .zip(region)
            .map(|(resource, region)| NodeScheduler {
                similarity: node.scheduler_similarity,
                resource,
                region: Some(region.clone()),
            });

        // If there is a scheduler we return the node placement variant,
        // otherwise we return the host id variant.
        let placement = scheduler.map(api::NodeScheduler::new).map_or_else(
            || api::node_placement::Placement::HostId(node.host_id.to_string()),
            api::node_placement::Placement::Scheduler,
        );

        let block_height = node
            .block_height
            .map(u64::try_from)
            .transpose()
            .map_err(Error::BlockHeight)?;
        let staking_status = node
            .staking_status
            .map(api::StakingStatus::from_model)
            .map(Into::into);

        let allow_ips = node
            .allow_ips()?
            .into_iter()
            .map(api::FilteredIpAddr::from_model)
            .collect();
        let deny_ips = node
            .deny_ips()?
            .into_iter()
            .map(api::FilteredIpAddr::from_model)
            .collect();

        let jobs = node.jobs()?;
        let jobs = jobs.into_iter().map(api::NodeJob::from_model).collect();

        Ok(api::Node {
            id: node.id.to_string(),
            org_id: node.org_id.to_string(),
            host_id: node.host_id.to_string(),
            host_name: node.host_name,
            blockchain_id: node.blockchain_id.to_string(),
            name: node.name,
            address: node.address,
            version: node.version.into(),
            ip: node.ip_addr,
            ip_gateway: node.ip_gateway,
            node_type: api::NodeType::from(node.node_type).into(),
            properties,
            block_height,
            created_at: Some(NanosUtc::from(node.created_at).into()),
            updated_at: Some(NanosUtc::from(node.updated_at).into()),
            status: api::NodeStatus::from_model(node.node_status).into(),
            staking_status,
            container_status: api::ContainerStatus::from_model(node.container_status).into(),
            sync_status: api::SyncStatus::from_model(node.sync_status).into(),
            self_update: node.self_update,
            network: node.network,
            blockchain_name: blockchain.name.clone(),
            created_by,
            allow_ips,
            deny_ips,
            placement: Some(api::NodePlacement {
                placement: Some(placement),
            }),
            org_name: org.name.clone(),
            host_org_id: host.org_id.to_string(),
            data_directory_mountpoint: node.data_directory_mountpoint,
            jobs,
        })
    }
}

impl api::NodeServiceCreateRequest {
    pub async fn as_new(
        &self,
        requirements: HardwareRequirements,
        created_by: Resource,
        conn: &mut Conn<'_>,
    ) -> Result<NewNode, Error> {
        let inner = self.placement.as_ref().ok_or(Error::MissingPlacement)?;
        let placement = inner.placement.as_ref().ok_or(Error::MissingPlacement)?;
        let scheduler = match placement {
            api::node_placement::Placement::HostId(_) => None,
            api::node_placement::Placement::Scheduler(s) => Some(s),
        };

        let allow_ips: Vec<FilteredIpAddr> = self
            .allow_ips
            .iter()
            .map(api::FilteredIpAddr::as_model)
            .collect();
        let deny_ips: Vec<FilteredIpAddr> = self
            .deny_ips
            .iter()
            .map(api::FilteredIpAddr::as_model)
            .collect();

        let region = scheduler.map(|s| &s.region);
        let region = region.map(|id| Region::by_name(id, conn));
        let region = OptionFuture::from(region).await.transpose()?;

        let entry = ResourceEntry::from(created_by);

        Ok(NewNode {
            id: Uuid::new_v4().into(),
            org_id: self.org_id.parse().map_err(Error::ParseOrgId)?,
            name: Petnames::large()
                .generate_one(3, "_")
                .ok_or(Error::GeneratePetnames)?,
            version: self.version.clone().into(),
            blockchain_id: self
                .blockchain_id
                .parse()
                .map_err(Error::ParseBlockchainId)?,
            block_height: None,
            node_data: None,
            node_status: NodeStatus::ProvisioningPending,
            sync_status: NodeSyncStatus::Unknown,
            staking_status: NodeStakingStatus::Unknown,
            container_status: ContainerStatus::Unknown,
            self_update: true,
            vcpu_count: requirements.vcpu_count.try_into().map_err(Error::Vcpu)?,
            mem_size_bytes: (requirements.mem_size_mb * 1000 * 1000)
                .try_into()
                .map_err(Error::MemSize)?,
            disk_size_bytes: (requirements.disk_size_gb * 1000 * 1000 * 1000)
                .try_into()
                .map_err(Error::DiskSize)?,
            network: self.network.clone().into(),
            node_type: self.node_type().into(),
            allow_ips: serde_json::to_value(allow_ips).map_err(Error::AllowIps)?,
            deny_ips: serde_json::to_value(deny_ips).map_err(Error::DenyIps)?,
            created_by: entry.resource_id,
            created_by_resource: entry.resource_type,
            // We use and_then here to coalesce the scheduler being None and the similarity being
            // None. This is because both the scheduler and the similarity are optional.
            scheduler_similarity: scheduler.and_then(|s| s.similarity().into_model()),
            // Here we use `map` and `transpose`, because the scheduler is optional, but if it is
            // provided, the `resource` is not optional.
            scheduler_resource: scheduler
                .map(|s| s.resource().into_model().ok_or(Error::NoResourceAffinity))
                .transpose()?,
            scheduler_region: region.map(|r| r.id),
        })
    }

    fn host_id(&self) -> Result<Option<HostId>, Error> {
        let inner = self.placement.as_ref().ok_or(Error::MissingPlacement)?;
        let placement = inner.placement.as_ref().ok_or(Error::MissingPlacement)?;

        match placement {
            api::node_placement::Placement::Scheduler(_) => Ok(None),
            api::node_placement::Placement::HostId(id) => {
                Ok(Some(id.parse().map_err(Error::ParseHostId)?))
            }
        }
    }

    fn properties(
        &self,
        node: &Node,
        name_to_id_map: &HashMap<String, BlockchainPropertyId>,
    ) -> Result<Vec<NodeProperty>, Error> {
        self.properties
            .iter()
            .map(|prop| {
                Ok(NodeProperty {
                    id: Uuid::new_v4().into(),
                    node_id: node.id,
                    blockchain_property_id: name_to_id_map
                        .get(&prop.name)
                        .copied()
                        .ok_or_else(|| Error::PropertyNotFound(prop.name.clone()))?,
                    value: prop.value.clone(),
                })
            })
            .collect()
    }
}

impl api::NodeServiceListRequest {
    fn as_filter(&self) -> Result<NodeFilter, Error> {
        Ok(NodeFilter {
            org_id: self
                .org_id
                .as_ref()
                .map(|id| id.parse().map_err(Error::ParseOrgId))
                .transpose()?,
            offset: self.offset,
            limit: self.limit,
            status: self
                .statuses()
                .map(|status| status.into_model().ok_or(Error::NodeStatusUnspecified))
                .collect::<Result<_, _>>()?,
            node_types: self.node_types().map(NodeType::from).collect(),
            blockchains: self
                .blockchain_ids
                .iter()
                .map(|id| id.parse().map_err(Error::ParseBlockchainId))
                .collect::<Result<_, _>>()?,
            host_id: self
                .host_id
                .as_ref()
                .map(|id| id.parse().map_err(Error::ParseHostId))
                .transpose()?,
            search: self
                .search
                .as_ref()
                .map(|s| {
                    s.operator().try_into().map(|operator| NodeSearch {
                        operator,
                        id: s.id.as_ref().map(|id| id.trim().to_lowercase()),
                        name: s.name.as_ref().map(|name| name.trim().to_lowercase()),
                        ip: s.ip.as_ref().map(|ip| ip.trim().to_lowercase()),
                    })
                })
                .transpose()
                .map_err(Error::SearchOperator)?,
        })
    }
}

impl api::NodeServiceUpdateConfigRequest {
    pub fn as_update(&self) -> Result<UpdateNode<'_>, Error> {
        // Convert the ip list from the gRPC structures to the database models.
        let allow_ips: Vec<FilteredIpAddr> = self
            .allow_ips
            .iter()
            .map(api::FilteredIpAddr::as_model)
            .collect();
        let deny_ips: Vec<FilteredIpAddr> = self
            .deny_ips
            .iter()
            .map(api::FilteredIpAddr::as_model)
            .collect();

        Ok(UpdateNode {
            id: self.id.parse().map_err(Error::ParseId)?,
            name: None,
            version: None,
            ip_addr: None,
            block_height: None,
            node_data: None,
            node_status: None,
            sync_status: None,
            staking_status: None,
            container_status: None,
            self_update: self.self_update,
            address: None,
            allow_ips: Some(serde_json::to_value(allow_ips).map_err(Error::AllowIps)?),
            deny_ips: Some(serde_json::to_value(deny_ips).map_err(Error::DenyIps)?),
        })
    }
}

impl api::NodeServiceUpdateStatusRequest {
    pub fn as_update(&self) -> Result<UpdateNode<'_>, Error> {
        Ok(UpdateNode {
            id: self.id.parse().map_err(Error::ParseId)?,
            name: None,
            version: self.version.as_deref(),
            ip_addr: None,
            block_height: None,
            node_data: None,
            node_status: None,
            sync_status: None,
            staking_status: None,
            container_status: Some(self.container_status().into_model()),
            self_update: None,
            address: self.address.as_deref(),
            allow_ips: None,
            deny_ips: None,
        })
    }
}

impl api::NodeProperty {
    fn from_model(model: NodeProperty, bprop: BlockchainProperty) -> Self {
        let mut prop = api::NodeProperty {
            name: bprop.name,
            display_name: bprop.display_name,
            ui_type: 0,
            disabled: bprop.disabled,
            required: bprop.required,
            value: model.value,
        };
        prop.set_ui_type(api::UiType::from(bprop.ui_type));
        prop
    }
}

impl api::NodeScheduler {
    fn new(node: NodeScheduler) -> Self {
        use api::node_scheduler::{ResourceAffinity, SimilarNodeAffinity};

        let mut scheduler = Self {
            similarity: None,
            resource: 0,
            region: node.region.map(|r| r.name).unwrap_or_default(),
        };
        scheduler.set_resource(ResourceAffinity::from_model(node.resource));
        if let Some(similarity) = node.similarity {
            scheduler.set_similarity(SimilarNodeAffinity::from_model(similarity));
        }
        scheduler
    }
}

impl api::FilteredIpAddr {
    fn from_model(model: FilteredIpAddr) -> Self {
        Self {
            ip: model.ip,
            description: model.description,
        }
    }

    fn as_model(&self) -> FilteredIpAddr {
        FilteredIpAddr {
            ip: self.ip.clone(),
            description: self.description.clone(),
        }
    }
}

impl api::NodeJob {
    pub fn into_model(self) -> NodeJob {
        let status = self.status().into_model();
        NodeJob {
            name: self.name,
            status,
            exit_code: self.exit_code,
            message: self.message,
            logs: self.logs,
            restarts: self.restarts,
            progress: self.progress.map(api::NodeJobProgress::into_model),
        }
    }

    pub fn from_model(model: NodeJob) -> Self {
        let mut node_job = Self {
            name: model.name,
            status: 0,
            exit_code: model.exit_code,
            message: model.message,
            logs: model.logs,
            restarts: model.restarts,
            progress: model.progress.map(api::NodeJobProgress::from_model),
        };
        if let Some(status) = model.status {
            node_job.set_status(api::NodeJobStatus::from_model(status));
        }
        node_job
    }
}

impl api::NodeJobProgress {
    pub fn into_model(self) -> NodeJobProgress {
        NodeJobProgress {
            total: self.total,
            current: self.current,
            message: self.message,
        }
    }

    fn from_model(model: NodeJobProgress) -> Self {
        Self {
            total: model.total,
            current: model.current,
            message: model.message,
        }
    }
}

impl api::NodeJobStatus {
    pub const fn into_model(self) -> Option<NodeJobStatus> {
        match self {
            Self::Unspecified => None,
            Self::Pending => Some(NodeJobStatus::Pending),
            Self::Running => Some(NodeJobStatus::Running),
            Self::Finished => Some(NodeJobStatus::Finished),
            Self::Failed => Some(NodeJobStatus::Failed),
            Self::Stopped => Some(NodeJobStatus::Stopped),
        }
    }

    const fn from_model(model: NodeJobStatus) -> Self {
        match model {
            NodeJobStatus::Pending => Self::Pending,
            NodeJobStatus::Running => Self::Running,
            NodeJobStatus::Finished => Self::Finished,
            NodeJobStatus::Failed => Self::Failed,
            NodeJobStatus::Stopped => Self::Stopped,
        }
    }
}

impl common::EntityUpdate {
    pub async fn from_node(node: &Node, conn: &mut Conn<'_>) -> Result<Option<Self>, Error> {
        let (Some(created_by), Some(resource)) = (node.created_by, node.created_by_resource) else {
            return Ok(None);
        };

        let user = if resource == ResourceType::User {
            Some(User::find_by_id((*created_by).into(), conn).await?)
        } else {
            None
        };

        Ok(Self::from_node_user(node, user.as_ref()))
    }

    pub fn from_node_user(node: &Node, user: Option<&User>) -> Option<Self> {
        let (Some(created_by), Some(resource)) = (node.created_by, node.created_by_resource) else {
            return None;
        };

        Some(common::EntityUpdate {
            resource: common::Resource::from(resource).into(),
            resource_id: Some(created_by.to_string()),
            name: user.map(User::name),
            email: user.map(|u| u.email.clone()),
        })
    }
}
