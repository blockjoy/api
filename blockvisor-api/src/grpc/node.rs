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
use crate::auth::resource::{
    HostId, NodeId, OrgId, Resource, ResourceEntry, ResourceId, ResourceType, UserId,
};
use crate::auth::{AuthZ, Authorize};
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::models::blockchain::{
    BlockchainNodeType, BlockchainProperty, BlockchainPropertyId, BlockchainVersion,
};
use crate::models::command::NewCommand;
use crate::models::node::{
    self, ContainerStatus, FilteredIpAddr, NewNode, Node, NodeFilter, NodeJob, NodeJobProgress,
    NodeJobStatus, NodeProperty, NodeReport, NodeScheduler, NodeSearch, NodeSort, NodeStatus,
    NodeType, NodeVersion, StakingStatus, SyncStatus, UpdateNode,
};
use crate::models::{Blockchain, CommandType, Host, Org, Region, User};
use crate::storage::image::ImageId;
use crate::storage::metadata::HardwareRequirements;
use crate::util::{HashVec, NanosUtc};

use super::api::node_service_server::NodeService;
use super::{api, common, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse allow ips: {0}
    AllowIps(serde_json::Error),
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Auth token parsing failed: {0}
    AuthToken(#[from] crate::auth::token::Error),
    /// Node blockchain error: {0}
    Blockchain(#[from] crate::models::blockchain::Error),
    /// Node blockchain node type error: {0}
    BlockchainNodeType(#[from] crate::models::blockchain::node_type::Error),
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
    /// Failed to parse deny ips: {0}
    DenyIps(serde_json::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Failed to parse disk size bytes: {0}
    DiskSize(std::num::TryFromIntError),
    /// Failed to generate node name. This should not happen.
    GenerateName,
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
    /// Node type model error: {0}
    NodeType(#[from] crate::models::node::node_type::Error),
    /// No ResourceAffinity.
    NoResourceAffinity,
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
    /// Unable to parse node version: {0}
    ParseNodeVersion(crate::models::node::node_type::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Blockchain property not found: {0}
    PropertyNotFound(String),
    /// Node region error: {0}
    Region(#[from] crate::models::region::Error),
    /// Node report error: {0}
    Report(#[from] crate::models::node::report::Error),
    /// Node resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// Node search failed: {0}
    SearchOperator(crate::util::search::Error),
    /// Sort order: {0}
    SortOrder(crate::util::search::Error),
    /// Node storage error: {0}
    Storage(#[from] crate::storage::Error),
    /// Failed to parse current data sync progress: {0}
    SyncCurrent(std::num::TryFromIntError),
    /// Failed to parse total data sync progress: {0}
    SyncTotal(std::num::TryFromIntError),
    /// The requested sort field is unknown.
    UnknownSortField,
    /// Attempt to update status by {1} {2} of node `{0}`, which doesn't exist.
    UpdateStatusMissingNode(NodeId, ResourceType, ResourceId),
    /// Node user error: {0}
    User(#[from] crate::models::user::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | GenerateName | MissingPropertyId(_) | ModelProperty(_) | ParseIpAddr(_)
            | PropertyNotFound(_) | Storage(_) => Status::internal("Internal error."),
            AllowIps(_) => Status::invalid_argument("allow_ips"),
            BlockHeight(_) => Status::invalid_argument("block_height"),
            DenyIps(_) => Status::invalid_argument("deny_ips"),
            DiskSize(_) => Status::invalid_argument("disk_size_bytes"),
            MemSize(_) => Status::invalid_argument("mem_size_bytes"),
            MissingPlacement => Status::invalid_argument("placement"),
            NoResourceAffinity => Status::invalid_argument("resource"),
            ParseBlockchainId(_) => Status::invalid_argument("blockchain_id"),
            ParseHostId(_) => Status::invalid_argument("host_id"),
            ParseId(_) => Status::invalid_argument("id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            SyncCurrent(_) => Status::invalid_argument("data_sync_progress_current"),
            SyncTotal(_) => Status::invalid_argument("data_sync_progress_total"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            UpdateStatusMissingNode(_, _, _) => Status::not_found("No such node"),
            Auth(err) => err.into(),
            AuthToken(err) => err.into(),
            Blockchain(err) => err.into(),
            BlockchainNodeType(err) => err.into(),
            BlockchainProperty(err) => err.into(),
            BlockchainVersion(err) => err.into(),
            Claims(err) => err.into(),
            Command(err) => err.into(),
            CommandGrpc(err) => err.into(),
            Host(err) => err.into(),
            IpAddress(err) => err.into(),
            Model(err) => err.into(),
            NodeType(err) => err.into(),
            Org(err) => err.into(),
            ParseNodeVersion(err) => err.into(),
            Region(err) => err.into(),
            Report(err) => err.into(),
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

    async fn upgrade(
        &self,
        req: Request<api::NodeServiceUpgradeRequest>,
    ) -> Result<Response<api::NodeServiceUpgradeResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| upgrade(req, meta, write).scope_boxed())
            .await
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

    async fn report(
        &self,
        req: Request<api::NodeServiceReportRequest>,
    ) -> Result<Response<api::NodeServiceReportResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| report(req, meta, write).scope_boxed())
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
    let node = Node::by_id(node_id, &mut read).await?;

    let authz = read
        .auth_or_all(&meta, NodeAdminPerm::Get, NodePerm::Get, node_id)
        .await?;

    Ok(api::NodeServiceGetResponse {
        node: Some(api::Node::from_model(node, &authz, &mut read).await?),
    })
}

async fn list(
    req: api::NodeServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::NodeServiceListResponse, Error> {
    let filter = req.into_filter()?;
    let authz = if filter.org_ids.is_empty() {
        read.auth_all(&meta, NodeAdminPerm::List).await?
    } else {
        read.auth_or_all(&meta, NodeAdminPerm::List, NodePerm::List, &filter.org_ids)
            .await?
    };

    let (nodes, node_count) = filter.query(&mut read).await?;
    let nodes = api::Node::from_models(nodes, &authz, &mut read).await?;

    Ok(api::NodeServiceListResponse { nodes, node_count })
}

async fn create(
    req: api::NodeServiceCreateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceCreateResponse, Error> {
    let org_id = req.org_id.parse().map_err(Error::ParseOrgId)?;

    // The host_id is either determined by the scheduler, or an optional host_id.
    let (host, authz) = if let Some(host_id) = req.host_id()? {
        let host = Host::by_id(host_id, &mut write).await?;
        let authz = write
            .auth_or_all(&meta, NodeAdminPerm::Create, NodePerm::Create, host_id)
            .await?;
        (Some(host), authz)
    } else if let Ok(authz) = write.auth_all(&meta, NodeAdminPerm::Create).await {
        (None, authz)
    } else {
        let authz = write.auth(&meta, NodePerm::Create, org_id).await?;
        (None, authz)
    };

    let blockchain_id = req
        .blockchain_id
        .parse()
        .map_err(Error::ParseBlockchainId)?;
    let blockchain = Blockchain::by_id(blockchain_id, &authz, &mut write).await?;

    let node_type = req.node_type().into();
    let image = ImageId::new(&blockchain.name, node_type, req.version.clone().into());
    let version =
        BlockchainVersion::find(blockchain_id, node_type, &image.node_version, &mut write).await?;

    let requirements = write.ctx.storage.rhai_metadata(&image).await?.requirements;
    let created_by = authz.resource();
    let new_node = req
        .as_new(requirements, org_id, created_by, &mut write)
        .await?;
    let node = new_node.create(host, &authz, &mut write).await?;

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

    let create_notif = NewCommand::node(&node, CommandType::NodeCreate)?
        .create(&mut write)
        .await?;
    let create_cmd = api::Command::from_model(&create_notif, &authz, &mut write).await?;
    let node_api = api::Node::from_model(node, &authz, &mut write).await?;

    let created_by = common::EntityUpdate::from_resource(created_by, &mut write).await?;
    let created = api::NodeMessage::created(node_api.clone(), created_by);

    write.mqtt(create_cmd);
    write.mqtt(created);

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
    let node = Node::by_id(node_id, &mut write).await?;

    let authz = if req.org_id.is_some() {
        let perms = [NodeAdminPerm::UpdateConfig, NodeAdminPerm::Transfer];
        write.auth_all(&meta, perms).await?
    } else {
        write
            .auth_or_all(
                &meta,
                NodeAdminPerm::UpdateConfig,
                NodePerm::UpdateConfig,
                node_id,
            )
            .await?
    };

    let node = node.update(req.as_update()?, &mut write).await?;
    let updated = NewCommand::node(&node, CommandType::NodeUpdate)?
        .create(&mut write)
        .await?;
    let cmd = api::Command::from_model(&updated, &authz, &mut write).await?;

    let node = api::Node::from_model(node, &authz, &mut write).await?;
    let updated_by = common::EntityUpdate::from_resource(&authz, &mut write).await?;
    let updated = api::NodeMessage::updated(node, updated_by);

    write.mqtt(cmd);
    write.mqtt(updated);

    Ok(api::NodeServiceUpdateConfigResponse {})
}

async fn upgrade(
    req: api::NodeServiceUpgradeRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceUpgradeResponse, Error> {
    let node_id: NodeId = req.id.parse().map_err(Error::ParseId)?;
    let node = Node::by_id(node_id, &mut write).await?;

    let authz = write
        .auth_or_all(&meta, NodeAdminPerm::Upgrade, NodePerm::Upgrade, node_id)
        .await?;

    let blockchain = Blockchain::by_id(node.blockchain_id, &authz, &mut write).await?;
    let node_type =
        BlockchainNodeType::by_node_type(blockchain.id, node.node_type, &authz, &mut write).await?;
    let new_version =
        BlockchainVersion::by_node_type_version(node_type.id, &req.version, &mut write).await?;

    // node.version = NodeVersion::new(&new_version.version);
    let update = UpdateNode {
        version: Some(NodeVersion::new(&new_version.version).map_err(Error::ParseNodeVersion)?),
        ..Default::default()
    };
    let node = node.update(update, &mut write).await?;

    let cmd = NewCommand::node(&node, CommandType::NodeUpgrade)?
        .create(&mut write)
        .await?;
    let cmd = api::Command::from_model(&cmd, &authz, &mut write).await?;

    write.mqtt(cmd);

    Ok(api::NodeServiceUpgradeResponse {})
}

async fn update_status(
    req: api::NodeServiceUpdateStatusRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceUpdateStatusResponse, Error> {
    let node_id: NodeId = req.id.parse().map_err(Error::ParseId)?;
    let node = match Node::by_id(node_id, &mut write).await {
        Err(node::Error::FindById(_, diesel::result::Error::NotFound)) => {
            let token = (&meta).try_into()?;
            let claims = write.ctx.auth.claims(&token, &mut write).await?;
            return Err(Error::UpdateStatusMissingNode(
                node_id,
                claims.resource_entry.resource_type,
                claims.resource_entry.resource_id,
            ));
        }
        Err(e) => return Err(e.into()),
        Ok(node) => node,
    };

    let authz = write
        .auth_or_all(
            &meta,
            NodeAdminPerm::UpdateStatus,
            NodePerm::UpdateStatus,
            node_id,
        )
        .await?;

    let node = node.update(req.as_update()?, &mut write).await?;
    let node = api::Node::from_model(node, &authz, &mut write).await?;

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
    let node = Node::by_id(node_id, &mut write).await?;

    let authz = write
        .auth_or_all(&meta, NodeAdminPerm::Delete, NodePerm::Delete, node_id)
        .await?;

    let update = UpdateNode {
        node_status: Some(NodeStatus::DeletePending),
        ..Default::default()
    };
    let node = node.update(update, &mut write).await?;
    Node::delete(node.id, &mut write).await?;

    // Send delete node command
    let new_command = NewCommand::node(&node, CommandType::NodeDelete)?;
    let cmd = new_command.create(&mut write).await?;
    let cmd = api::Command::from_model(&cmd, &authz, &mut write).await?;

    let deleted_by = common::EntityUpdate::from_resource(&authz, &mut write).await?;
    let deleted = api::NodeMessage::deleted(&node, Some(deleted_by));

    write.mqtt(cmd);
    write.mqtt(deleted);

    Ok(api::NodeServiceDeleteResponse {})
}

async fn report(
    req: api::NodeServiceReportRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceReportResponse, Error> {
    let node_id: NodeId = req.node_id.parse().map_err(Error::ParseId)?;

    let authz = write
        .auth_or_all(&meta, NodeAdminPerm::Report, NodePerm::Report, node_id)
        .await?;
    let node = Node::by_id(node_id, &mut write).await?;

    let resource = authz.resource();
    let report = node.report(resource, req.message, &mut write).await?;

    Ok(api::NodeServiceReportResponse {
        id: report.id.to_string(),
    })
}

async fn start(
    req: api::NodeServiceStartRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceStartResponse, Error> {
    let node_id: NodeId = req.id.parse().map_err(Error::ParseId)?;
    let node = Node::by_id(node_id, &mut write).await?;

    let authz = write
        .auth_or_all(&meta, NodeAdminPerm::Start, NodePerm::Start, node_id)
        .await?;

    let cmd = NewCommand::node(&node, CommandType::NodeRestart)?
        .create(&mut write)
        .await?;
    let cmd = api::Command::from_model(&cmd, &authz, &mut write).await?;

    write.mqtt(cmd);

    Ok(api::NodeServiceStartResponse {})
}

async fn stop(
    req: api::NodeServiceStopRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceStopResponse, Error> {
    let node_id = req.id.parse().map_err(Error::ParseId)?;
    let node = Node::by_id(node_id, &mut write).await?;

    let authz = write
        .auth_or_all(&meta, NodeAdminPerm::Stop, NodePerm::Stop, node_id)
        .await?;

    let cmd = NewCommand::node(&node, CommandType::NodeStop)?
        .create(&mut write)
        .await?;
    let cmd = api::Command::from_model(&cmd, &authz, &mut write).await?;

    write.mqtt(cmd);

    Ok(api::NodeServiceStopResponse {})
}

async fn restart(
    req: api::NodeServiceRestartRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceRestartResponse, Error> {
    let node_id = req.id.parse().map_err(Error::ParseId)?;
    let node = Node::by_id(node_id, &mut write).await?;

    let authz = write
        .auth_or_all(&meta, NodeAdminPerm::Restart, NodePerm::Restart, node_id)
        .await?;

    let cmd = NewCommand::node(&node, CommandType::NodeRestart)?
        .create(&mut write)
        .await?;
    let cmd = api::Command::from_model(&cmd, &authz, &mut write).await?;

    write.mqtt(cmd);

    Ok(api::NodeServiceRestartResponse {})
}

impl api::Node {
    /// This function is used to create a ui node from a database node. We want to include the
    /// `database_name` in the ui representation, but it is not in the node model. Therefore we
    /// perform a seperate query to the blockchains table.
    pub async fn from_model(node: Node, authz: &AuthZ, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let blockchain = Blockchain::by_id(node.blockchain_id, authz, conn).await?;

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

        let host = Host::by_id(node.host_id, conn).await?;
        let org = Org::by_id(node.org_id, conn).await?;
        let region = node.region(conn).await?;
        let reports = NodeReport::by_node(node.id, conn).await?;
        let user_ids = reports
            .iter()
            .filter_map(NodeReport::user_id)
            .chain(node.created_by_user())
            .collect();
        let users = User::by_ids(user_ids, conn)
            .await?
            .to_map_keep_last(|u| (u.id, u));

        api::Node::new(
            node,
            &org,
            &host,
            &blockchain,
            properties,
            region.as_ref(),
            reports,
            &users,
        )
    }

    /// This function is used to create many ui nodes from many database nodes. The same
    /// justification as above applies. Note that this function does not simply defer to the
    /// function above, but rather it performs 1 query for n nodes. We like it this way :)
    pub async fn from_models(
        nodes: Vec<Node>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let node_ids = nodes.iter().map(|n| n.id).collect();
        let node_props = NodeProperty::by_node_ids(&node_ids, conn).await?;
        let property_ids = node_props
            .iter()
            .map(|np| np.blockchain_property_id)
            .collect();

        let blockchain_ids = nodes.iter().map(|n| n.blockchain_id).collect();
        let blockchains = Blockchain::by_ids(blockchain_ids, authz, conn)
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
        let orgs = Org::by_ids(org_ids, conn)
            .await?
            .to_map_keep_last(|org| (org.id, org));

        let host_ids = nodes.iter().map(|n| n.host_id).collect();
        let hosts = Host::by_ids(host_ids, conn)
            .await?
            .to_map_keep_last(|host| (host.id, host));

        let region_ids = nodes.iter().filter_map(|n| n.scheduler_region).collect();
        let regions = Region::by_ids(region_ids, conn)
            .await?
            .to_map_keep_last(|region| (region.id, region));

        let mut reports = NodeReport::by_node_ids(&node_ids, conn)
            .await?
            .to_map_keep_all(|report| (report.node_id, report));

        let user_ids = nodes
            .iter()
            .filter_map(Node::created_by_user)
            .chain(reports.values().flatten().filter_map(NodeReport::user_id))
            .collect();
        let users = User::by_ids(user_ids, conn)
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
                let reports = reports.remove(&node.id).unwrap_or_default();

                Some(api::Node::new(
                    node, org, host, blockchain, properties, region, reports, &users,
                ))
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)] // not now please
    pub fn new(
        node: Node,
        org: &Org,
        host: &Host,
        blockchain: &Blockchain,
        properties: Vec<api::NodeProperty>,
        region: Option<&Region>,
        reports: Vec<NodeReport>,
        users: &HashMap<UserId, User>,
    ) -> Result<Self, Error> {
        let scheduler = node
            .scheduler_resource
            .zip(region)
            .map(|(resource, region)| NodeScheduler {
                similarity: node.scheduler_similarity,
                resource,
                region: Some(region.clone()),
            });

        let user = node.created_by.and_then(|id| users.get(&(*id).into()));
        let created_by = common::EntityUpdate::from_node_user(&node, user);

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
            .map(common::StakingStatus::from)
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
            host_name: host.name.clone(),
            blockchain_id: node.blockchain_id.to_string(),
            name: node.name,
            address: node.address,
            version: node.version.into(),
            ip: node.ip_addr,
            ip_gateway: node.ip_gateway,
            node_type: common::NodeType::from(node.node_type).into(),
            properties,
            block_height,
            created_at: Some(NanosUtc::from(node.created_at).into()),
            updated_at: Some(NanosUtc::from(node.updated_at).into()),
            status: common::NodeStatus::from(node.node_status).into(),
            staking_status,
            container_status: common::ContainerStatus::from(node.container_status).into(),
            sync_status: common::SyncStatus::from(node.sync_status).into(),
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
            reports: reports
                .into_iter()
                .map(|report| {
                    let created_by = report.user_id().and_then(|id| users.get(&id));
                    api::NodeReport {
                        id: report.id.to_string(),
                        message: report.message,
                        created_by: Some(common::EntityUpdate {
                            resource: common::Resource::from(report.created_by_resource) as i32,
                            resource_id: Some(report.created_by.to_string()),
                            name: created_by.map(User::name),
                            email: created_by.map(|u| u.email.clone()),
                        }),
                        created_at: Some(NanosUtc::from(report.created_at).into()),
                    }
                })
                .collect(),
            note: node.note,
            url: node.url.to_string(),
        })
    }
}

impl api::NodeServiceCreateRequest {
    pub async fn as_new(
        &self,
        requirements: HardwareRequirements,
        org_id: OrgId,
        created_by: Resource,
        conn: &mut Conn<'_>,
    ) -> Result<NewNode, Error> {
        let name = Petnames::small()
            .generate_one(3, "-")
            .ok_or(Error::GenerateName)?;
        let placement = self
            .placement
            .as_ref()
            .and_then(|p| p.placement.as_ref())
            .ok_or(Error::MissingPlacement)?;
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
            org_id,
            name,
            version: self.version.clone().into(),
            blockchain_id: self
                .blockchain_id
                .parse()
                .map_err(Error::ParseBlockchainId)?,
            block_height: None,
            node_data: None,
            node_status: NodeStatus::ProvisioningPending,
            sync_status: SyncStatus::Unknown,
            staking_status: StakingStatus::Unknown,
            container_status: ContainerStatus::Unknown,
            self_update: true,
            vcpu_count: requirements.vcpu_count.into(),
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
                let blockchain_property_id = name_to_id_map
                    .get(&prop.name)
                    .copied()
                    .ok_or_else(|| Error::PropertyNotFound(prop.name.clone()))?;

                Ok(NodeProperty {
                    id: Uuid::new_v4().into(),
                    node_id: node.id,
                    blockchain_property_id,
                    value: prop.value.clone(),
                })
            })
            .collect()
    }
}

impl api::NodeServiceListRequest {
    fn into_filter(self) -> Result<NodeFilter, Error> {
        let org_ids = self
            .org_ids
            .iter()
            .map(|id| id.parse().map_err(Error::ParseOrgId))
            .collect::<Result<_, _>>()?;
        let status = self.statuses().map(NodeStatus::from).collect();
        let node_types = self.node_types().map(NodeType::from).collect();
        let blockchain_ids = self
            .blockchain_ids
            .iter()
            .map(|id| id.parse().map_err(Error::ParseBlockchainId))
            .collect::<Result<_, _>>()?;
        let host_ids = self
            .host_ids
            .iter()
            .map(|id| id.parse().map_err(Error::ParseHostId))
            .collect::<Result<_, _>>()?;
        let user_ids = self
            .user_ids
            .iter()
            .map(|id| id.parse().map_err(Error::ParseHostId))
            .collect::<Result<_, _>>()?;
        let search = self
            .search
            .map(|search| {
                Ok::<_, Error>(NodeSearch {
                    operator: search
                        .operator()
                        .try_into()
                        .map_err(Error::SearchOperator)?,
                    id: search.id.map(|id| id.trim().to_lowercase()),
                    name: search.name.map(|name| name.trim().to_lowercase()),
                    ip: search.ip.map(|ip| ip.trim().to_lowercase()),
                })
            })
            .transpose()?;
        let sort = self
            .sort
            .into_iter()
            .map(|sort| {
                let order = sort.order().try_into().map_err(Error::SortOrder)?;
                match sort.field() {
                    api::NodeSortField::Unspecified => Err(Error::UnknownSortField),
                    api::NodeSortField::HostName => Ok(NodeSort::HostName(order)),
                    api::NodeSortField::NodeName => Ok(NodeSort::NodeName(order)),
                    api::NodeSortField::NodeType => Ok(NodeSort::NodeType(order)),
                    api::NodeSortField::CreatedAt => Ok(NodeSort::CreatedAt(order)),
                    api::NodeSortField::UpdatedAt => Ok(NodeSort::UpdatedAt(order)),
                    api::NodeSortField::NodeStatus => Ok(NodeSort::NodeStatus(order)),
                    api::NodeSortField::SyncStatus => Ok(NodeSort::SyncStatus(order)),
                    api::NodeSortField::ContainerStatus => Ok(NodeSort::ContainerStatus(order)),
                    api::NodeSortField::StakingStatus => Ok(NodeSort::StakingStatus(order)),
                }
            })
            .collect::<Result<_, _>>()?;

        Ok(NodeFilter {
            org_ids,
            offset: self.offset,
            limit: self.limit,
            status,
            node_types,
            blockchain_ids,
            host_ids,
            user_ids,
            ip_addresses: self.ip_addresses,
            versions: self.versions,
            networks: self.networks,
            regions: self.regions,
            search,
            sort,
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
            org_id: self
                .org_id
                .as_deref()
                .map(str::parse)
                .transpose()
                .map_err(Error::ParseOrgId)?,
            host_id: None,
            name: None,
            version: None,
            ip_addr: None,
            ip_gateway: None,
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
            note: self.note.as_deref(),
        })
    }
}

impl api::NodeServiceUpdateStatusRequest {
    pub fn as_update(&self) -> Result<UpdateNode<'_>, Error> {
        Ok(UpdateNode {
            org_id: None,
            host_id: None,
            name: None,
            version: self.version.as_deref().map(NodeVersion::new).transpose()?,
            ip_addr: None,
            ip_gateway: None,
            block_height: None,
            node_data: None,
            node_status: None,
            sync_status: None,
            staking_status: None,
            container_status: Some(self.container_status().into()),
            self_update: None,
            address: self.address.as_deref(),
            allow_ips: None,
            deny_ips: None,
            note: None,
        })
    }
}

impl api::NodeProperty {
    fn from_model(node_prop: NodeProperty, blockchain_prop: BlockchainProperty) -> Self {
        api::NodeProperty {
            name: blockchain_prop.name,
            display_name: blockchain_prop.display_name,
            ui_type: common::UiType::from(blockchain_prop.ui_type).into(),
            disabled: blockchain_prop.disabled,
            required: blockchain_prop.required,
            value: node_prop.value,
        }
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
