use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;
use futures_util::future::OptionFuture;

use crate::auth::claims::Claims;
use crate::auth::endpoint::Endpoint;
use crate::auth::resource::{HostId, NodeId, Resource, UserId};
use crate::config::Context;
use crate::cookbook::script::HardwareRequirements;
use crate::database::{Conn, Transaction};
use crate::models::blockchain::{BlockchainProperty, BlockchainPropertyUiType};
use crate::models::command::NewCommand;
use crate::models::node::{FilteredIpAddr, NewNode, NodeFilter, UpdateNode};
use crate::models::{
    Blockchain, Command, CommandType, ContainerStatus, Host, IpAddress, Node, NodeChainStatus,
    NodeProperty, NodeScheduler, NodeStakingStatus, NodeSyncStatus, NodeType, Org,
    ResourceAffinity, SimilarNodeAffinity, User,
};
use crate::timestamp::NanosUtc;

use super::api::{self, node_service_server};
use super::{helpers, Grpc};

#[tonic::async_trait]
impl node_service_server::NodeService for Grpc {
    async fn create(
        &self,
        req: tonic::Request<api::NodeServiceCreateRequest>,
    ) -> super::Resp<api::NodeServiceCreateResponse> {
        self.context
            .write(|conn, ctx| create(req, conn, ctx).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }

    async fn get(
        &self,
        req: tonic::Request<api::NodeServiceGetRequest>,
    ) -> super::Resp<api::NodeServiceGetResponse> {
        self.context
            .read(|conn, ctx| get(req, conn, ctx).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: tonic::Request<api::NodeServiceListRequest>,
    ) -> super::Resp<api::NodeServiceListResponse> {
        self.context
            .read(|conn, ctx| list(req, conn, ctx).scope_boxed())
            .await
    }

    async fn update_config(
        &self,
        req: tonic::Request<api::NodeServiceUpdateConfigRequest>,
    ) -> super::Resp<api::NodeServiceUpdateConfigResponse> {
        self.context
            .write(|conn, ctx| update_config(req, conn, ctx).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }

    async fn update_status(
        &self,
        req: tonic::Request<api::NodeServiceUpdateStatusRequest>,
    ) -> super::Resp<api::NodeServiceUpdateStatusResponse> {
        self.context
            .write(|conn, ctx| update_status(req, conn, ctx).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }

    async fn delete(
        &self,
        req: tonic::Request<api::NodeServiceDeleteRequest>,
    ) -> super::Resp<api::NodeServiceDeleteResponse> {
        self.context
            .write(|conn, ctx| delete(req, conn, ctx).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }

    async fn start(
        &self,
        req: tonic::Request<api::NodeServiceStartRequest>,
    ) -> super::Resp<api::NodeServiceStartResponse> {
        self.context
            .write(|conn, ctx| start(req, conn, ctx).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }

    async fn stop(
        &self,
        req: tonic::Request<api::NodeServiceStopRequest>,
    ) -> super::Resp<api::NodeServiceStopResponse> {
        self.context
            .write(|conn, ctx| stop(req, conn, ctx).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }

    async fn restart(
        &self,
        req: tonic::Request<api::NodeServiceRestartRequest>,
    ) -> super::Resp<api::NodeServiceRestartResponse> {
        self.context
            .write(|conn, ctx| restart(req, conn, ctx).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }
}

async fn get(
    req: tonic::Request<api::NodeServiceGetRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::NodeServiceGetResponse> {
    let claims = ctx.claims(&req, Endpoint::NodeGet).await?;
    let req = req.into_inner();
    let node = Node::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => Org::is_member(user_id, node.org_id, conn).await?,
        Resource::Org(org_id) => node.org_id == org_id,
        Resource::Host(host_id) => node.host_id == host_id,
        Resource::Node(node_id) => node.id == node_id,
    };
    if !is_allowed {
        super::forbidden!("Access not allowed")
    }
    let resp = api::NodeServiceGetResponse {
        node: Some(api::Node::from_model(node, conn).await?),
    };
    Ok(tonic::Response::new(resp))
}

async fn list(
    req: tonic::Request<api::NodeServiceListRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::NodeServiceListResponse> {
    let claims = ctx.claims(&req, Endpoint::NodeList).await?;
    let filter = req.into_inner().as_filter()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => Org::is_member(user_id, filter.org_id, conn).await?,
        Resource::Org(org_id) => filter.org_id == org_id,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for nodes list");
    }
    let (node_count, nodes) = Node::filter(filter, conn).await?;
    let nodes = api::Node::from_models(nodes, conn).await?;
    let resp = api::NodeServiceListResponse { nodes, node_count };
    Ok(tonic::Response::new(resp))
}

async fn create(
    req: tonic::Request<api::NodeServiceCreateRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> crate::Result<super::Outcome<api::NodeServiceCreateResponse>> {
    let claims = ctx.claims(&req, Endpoint::NodeCreate).await?;
    let Resource::User(user_id) = claims.resource() else { super::forbidden!("Need user_id!") };

    let user = User::find_by_id(user_id, conn).await?;
    let req = req.into_inner();
    let blockchain = Blockchain::find_by_id(req.blockchain_id.parse()?, conn).await?;
    // We want to cast a string like `NODE_TYPE_VALIDATOR` to `validator`.
    let node_type = &req.node_type().as_str_name()[10..];
    let reqs = ctx
        .cookbook
        .rhai_metadata(&blockchain.name, node_type, &req.version)
        .await?
        .requirements;
    let new_node = req.as_new(user.id, reqs)?;
    // The host_id will either be determined by the scheduler, or by the host_id.
    // Therfore we pass in an optional host_id for the node creation to fall back on if
    // there is no scheduler.
    let host_id = req.host_id()?;
    let host = if let Some(host_id) = host_id {
        let host = Host::find_by_id(host_id, conn).await?;
        if !Org::is_member(user.id, host.org_id, conn).await? {
            super::forbidden!("Must be member of org");
        }
        Some(host)
    } else {
        None
    };
    let node = new_node.create(host, conn, ctx).await?;
    // The user sends in the properties in a key-value style, that is,
    // { property name: property value }. We want to store this as
    // { property id: property value }. In order to map property names to property ids we can use
    // the id to name map, and then flip the keys and values to create an id to name map. Note that
    // this requires the names to be unique, but we expect this to be the case.
    let name_to_id_map =
        BlockchainProperty::id_to_name_map(&blockchain, node.node_type, &node.version, conn)
            .await?
            .into_iter()
            .map(|(k, v)| (v, k))
            .collect();
    NodeProperty::bulk_create(req.properties(&node, name_to_id_map)?, conn).await?;
    let create_notif = create_node_command(&node, CommandType::CreateNode, conn).await?;
    let create_cmd = api::Command::from_model(&create_notif, conn).await?;
    let start_notif = create_node_command(&node, CommandType::RestartNode, conn).await?;
    let start_cmd = api::Command::from_model(&start_notif, conn).await?;
    let node_api = api::Node::from_model(node, conn).await?;
    let created = api::NodeMessage::created(node_api.clone(), user.clone());
    let resp = api::NodeServiceCreateResponse {
        node: Some(node_api),
    };
    Ok(super::Outcome::new(resp)
        .with_msg(create_cmd)
        .with_msg(created)
        .with_msg(start_cmd))
}

async fn update_config(
    req: tonic::Request<api::NodeServiceUpdateConfigRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> crate::Result<super::Outcome<api::NodeServiceUpdateConfigResponse>> {
    let claims = ctx.claims(&req, Endpoint::NodeUpdateConfig).await?;
    let req = req.into_inner();
    let node = Node::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => Org::is_member(user_id, node.org_id, conn).await?,
        Resource::Org(org_id) => org_id == node.org_id,
        Resource::Host(host_id) => host_id == node.host_id,
        Resource::Node(node_id) => node_id == node.id,
    };
    if !is_allowed {
        super::forbidden!("Access not allowed")
    }
    let update_node = req.as_update()?;
    let user = claims
        .resource()
        .user()
        .map(|id| User::find_by_id(id, conn));
    let user = OptionFuture::from(user).await.transpose()?;
    let node = update_node.update(conn).await?;
    let create_notif = create_node_command(&node, CommandType::UpdateNode, conn).await?;
    let cmd = api::Command::from_model(&create_notif, conn).await?;
    let msg = api::NodeMessage::updated(node, user, conn).await?;
    let resp = api::NodeServiceUpdateConfigResponse {};
    Ok(super::Outcome::new(resp).with_msg(cmd).with_msg(msg))
}

async fn update_status(
    req: tonic::Request<api::NodeServiceUpdateStatusRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> crate::Result<super::Outcome<api::NodeServiceUpdateStatusResponse>> {
    let claims = ctx.claims(&req, Endpoint::NodeUpdateStatus).await?;
    let req = req.into_inner();
    let node = Node::find_by_id(req.id.parse()?, conn).await?;
    if !matches!(claims.resource(), Resource::Host(host_id) if node.host_id == host_id) {
        super::forbidden!("Access not allowed - only host owning node may update its status")
    }
    let update_node = req.as_update()?;
    let user = claims
        .resource()
        .user()
        .map(|id| User::find_by_id(id, conn));
    let user = OptionFuture::from(user).await.transpose()?;
    let node = update_node.update(conn).await?;
    let node_message = api::NodeMessage::updated(node, user, conn).await?;
    let resp = api::NodeServiceUpdateStatusResponse {};
    Ok(super::Outcome::new(resp).with_msg(node_message))
}

async fn delete(
    req: tonic::Request<api::NodeServiceDeleteRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> crate::Result<super::Outcome<api::NodeServiceDeleteResponse>> {
    let claims = ctx.claims(&req, Endpoint::NodeDelete).await?;
    let Resource::User(user_id) = claims.resource() else { super::forbidden!("Need user_id!") };
    let req = req.into_inner();
    let node = Node::find_by_id(req.id.parse()?, conn).await?;

    if !Org::is_member(user_id, node.org_id, conn).await? {
        super::forbidden!("User cannot delete node");
    }
    // 1. Delete node, if the node belongs to the current user
    // Key files are deleted automatically because of 'on delete cascade' in tables DDL
    Node::delete(node.id, conn, ctx).await?;

    let host_id = node.host_id;
    // 2. Do NOT delete reserved IP addresses, but set assigned to false
    let ip_addr = node.ip_addr.parse()?;
    let ip = IpAddress::find_by_node(ip_addr, conn).await?;

    IpAddress::unassign(ip.id, host_id, conn).await?;

    // Delete all pending commands for this node: there are not useable anymore
    Command::delete_pending(node.id, conn).await?;

    // Send delete node command
    let node_id = node.id.to_string();

    let new_command = NewCommand {
        host_id: node.host_id,
        cmd: CommandType::DeleteNode,
        sub_cmd: Some(&node_id),
        // Note that the `node_id` goes into the `sub_cmd` field, not the node_id field, because the
        // node was just deleted.
        node_id: None,
    };
    let cmd = new_command.create(conn).await?;

    let user = User::find_by_id(user_id, conn).await?;

    let cmd = api::Command::from_model(&cmd, conn).await?;

    let deleted = api::NodeMessage::deleted(node, user);
    let resp = api::NodeServiceDeleteResponse {};

    Ok(super::Outcome::new(resp).with_msg(cmd).with_msg(deleted))
}

async fn start(
    req: tonic::Request<api::NodeServiceStartRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> crate::Result<super::Outcome<api::NodeServiceStartResponse>> {
    let claims = ctx.claims(&req, Endpoint::NodeStart).await?;
    let req = req.into_inner();
    change_node_state(&req.id, CommandType::RestartNode, claims, conn).await
}

async fn stop(
    req: tonic::Request<api::NodeServiceStopRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> crate::Result<super::Outcome<api::NodeServiceStopResponse>> {
    let claims = ctx.claims(&req, Endpoint::NodeStop).await?;
    let req = req.into_inner();
    change_node_state(&req.id, CommandType::KillNode, claims, conn).await
}

async fn restart(
    req: tonic::Request<api::NodeServiceRestartRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> crate::Result<super::Outcome<api::NodeServiceRestartResponse>> {
    let claims = ctx.claims(&req, Endpoint::NodeRestart).await?;
    let req = req.into_inner();
    change_node_state(&req.id, CommandType::RestartNode, claims, conn).await
}

async fn change_node_state<Res: Default>(
    id: &str,
    cmd_type: CommandType,
    claims: Claims,
    conn: &mut Conn<'_>,
) -> crate::Result<super::Outcome<Res>> {
    let node = Node::find_by_id(id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => Org::is_member(user_id, node.org_id, conn).await?,
        Resource::Org(org_id) => org_id == node.org_id,
        Resource::Host(host_id) => host_id == node.host_id,
        Resource::Node(node_id) => node_id == node.id,
    };
    if !is_allowed {
        super::forbidden!("Access not allowed")
    }
    let create_notif = create_node_command(&node, cmd_type, conn).await?;
    let cmd = api::Command::from_model(&create_notif, conn).await?;

    Ok(super::Outcome::new(Default::default()).with_msg(cmd))
}

impl api::Node {
    /// This function is used to create a ui node from a database node. We want to include the
    /// `database_name` in the ui representation, but it is not in the node model. Therefore we
    /// perform a seperate query to the blockchains table.
    pub async fn from_model(node: Node, conn: &mut Conn<'_>) -> crate::Result<Self> {
        let blockchain = Blockchain::find_by_id(node.blockchain_id, conn).await?;
        let user_fut = node.created_by.map(|u_id| User::find_by_id(u_id, conn));
        let user = OptionFuture::from(user_fut).await.transpose()?;

        // We need to get both the node properties and the blockchain properties to construct the
        // final dto. First we query both, and then we zip them together.
        let nprops = NodeProperty::by_node(&node, conn).await?;
        let bprops = BlockchainProperty::by_node_props(&nprops, conn).await?;
        let bprops: HashMap<_, _> = bprops.into_iter().map(|prop| (prop.id, prop)).collect();
        let props = nprops
            .into_iter()
            .map(|nprop| (nprop.blockchain_property_id, nprop))
            .map(|(blockchain_property_id, nprop)| (nprop, bprops[&blockchain_property_id].clone()))
            .collect();

        let host = Host::find_by_id(node.host_id, conn).await?;
        let org = Org::find_by_id(node.org_id, conn).await?;

        Self::new(node, &blockchain, user.as_ref(), props, &org, &host)
    }

    /// This function is used to create many ui nodes from many database nodes. The same
    /// justification as above applies. Note that this function does not simply defer to the
    /// function above, but rather it performs 1 query for n nodes. We like it this way :)
    pub async fn from_models(nodes: Vec<Node>, conn: &mut Conn<'_>) -> crate::Result<Vec<Self>> {
        let blockchain_ids: Vec<_> = nodes.iter().map(|n| n.blockchain_id).collect();
        let blockchains: HashMap<_, _> = Blockchain::find_by_ids(&blockchain_ids, conn)
            .await?
            .into_iter()
            .map(|b| (b.id, b))
            .collect();
        let user_ids = nodes.iter().flat_map(|n| n.created_by).collect();
        let users: HashMap<_, _> = User::find_by_ids(user_ids, conn)
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();

        let nprops = NodeProperty::by_nodes(&nodes, conn).await?;
        let bprops = BlockchainProperty::by_node_props(&nprops, conn).await?;
        let bprops: HashMap<_, _> = bprops.into_iter().map(|prop| (prop.id, prop)).collect();
        let mut props_map: HashMap<NodeId, Vec<(_, _)>> = HashMap::new();
        for nprop in nprops {
            let blockchain_property_id = nprop.blockchain_property_id;
            props_map
                .entry(nprop.node_id)
                .or_default()
                .push((nprop, bprops[&blockchain_property_id].clone()));
        }

        let mut org_ids: Vec<_> = nodes.iter().map(|n| n.org_id).collect();
        org_ids.sort();
        org_ids.dedup();
        let orgs: HashMap<_, _> = Org::find_by_ids(org_ids, conn)
            .await?
            .into_iter()
            .map(|org| (org.id, org))
            .collect();

        let mut host_ids: Vec<_> = nodes.iter().map(|n| n.host_id).collect();
        host_ids.sort();
        host_ids.dedup();
        let hosts: HashMap<_, _> = Host::find_by_ids(host_ids, conn)
            .await?
            .into_iter()
            .map(|host| (host.id, host))
            .collect();

        nodes
            .into_iter()
            .map(|node| {
                Self::new(
                    node.clone(),
                    &blockchains[&node.blockchain_id],
                    node.created_by.and_then(|u_id| users.get(&u_id)),
                    props_map.get(&node.id).cloned().unwrap_or_default(),
                    &orgs[&node.org_id],
                    &hosts[&node.host_id],
                )
            })
            .collect()
    }

    /// Construct a new ui node from the queried parts.
    fn new(
        node: Node,
        blockchain: &Blockchain,
        user: Option<&User>,
        properties: Vec<(NodeProperty, BlockchainProperty)>,
        org: &Org,
        host: &Host,
    ) -> crate::Result<Self> {
        use api::{ContainerStatus, NodeStatus, NodeType, StakingStatus, SyncStatus};

        let properties = properties
            .into_iter()
            .map(|(nprop, bprop)| api::NodeProperty::from_model(nprop, bprop))
            .collect();

        let placement = node
            .scheduler()
            .map(api::NodeScheduler::new)
            // If there is a scheduler, we will return the scheduler variant of node placement.
            .map(api::node_placement::Placement::Scheduler)
            // If there isn't one, we return the host id variant.
            .unwrap_or_else(|| api::node_placement::Placement::HostId(node.host_id.to_string()));
        let placement = api::NodePlacement {
            placement: Some(placement),
        };

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

        let mut dto = Self {
            id: node.id.to_string(),
            org_id: node.org_id.to_string(),
            host_id: node.host_id.to_string(),
            host_name: node.host_name,
            blockchain_id: node.blockchain_id.to_string(),
            name: node.name,
            address: node.address,
            version: node.version,
            ip: node.ip_addr,
            ip_gateway: node.ip_gateway,
            node_type: 0, // We use the setter to set this field for type-safety
            properties,
            block_height: node.block_height.map(u64::try_from).transpose()?,
            created_at: Some(NanosUtc::from(node.created_at).into()),
            updated_at: Some(NanosUtc::from(node.updated_at).into()),
            status: 0,            // We use the setter to set this field for type-safety
            staking_status: None, // We use the setter to set this field for type-safety
            container_status: 0,  // We use the setter to set this field for type-safety
            sync_status: 0,       // We use the setter to set this field for type-safety
            self_update: node.self_update,
            network: node.network,
            blockchain_name: blockchain.name.clone(),
            created_by: user.map(|u| u.id.to_string()),
            created_by_name: user.map(|u| format!("{} {}", u.first_name, u.last_name)),
            created_by_email: user.map(|u| u.email.clone()),
            allow_ips,
            deny_ips,
            placement: Some(placement),
            org_name: org.name.clone(),
            host_org_id: host.org_id.to_string(),
        };
        dto.set_node_type(NodeType::from_model(node.node_type));
        dto.set_status(NodeStatus::from_model(node.chain_status));
        if let Some(ss) = node.staking_status {
            dto.set_staking_status(StakingStatus::from_model(ss));
        }
        dto.set_container_status(ContainerStatus::from_model(node.container_status));
        dto.set_sync_status(SyncStatus::from_model(node.sync_status));

        Ok(dto)
    }
}

impl api::NodeServiceCreateRequest {
    pub fn as_new(&self, user_id: UserId, req: HardwareRequirements) -> crate::Result<NewNode<'_>> {
        let placement = self
            .placement
            .as_ref()
            .ok_or_else(helpers::required("placement"))?
            .placement
            .as_ref()
            .ok_or_else(helpers::required("placement"))?;
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
        Ok(NewNode {
            id: uuid::Uuid::new_v4().into(),
            org_id: self.org_id.parse()?,
            name: petname::Petnames::large().generate_one(3, "_"),
            version: &self.version,
            blockchain_id: self.blockchain_id.parse()?,
            block_height: None,
            node_data: None,
            chain_status: NodeChainStatus::Provisioning,
            sync_status: NodeSyncStatus::Unknown,
            staking_status: NodeStakingStatus::Unknown,
            container_status: ContainerStatus::Unknown,
            self_update: false,
            vcpu_count: req.vcpu_count.try_into()?,
            mem_size_bytes: (req.mem_size_mb * 1000 * 1000).try_into()?,
            disk_size_bytes: (req.disk_size_gb * 1000 * 1000 * 1000).try_into()?,
            network: &self.network,
            node_type: self.node_type().into_model(),
            allow_ips: serde_json::to_value(allow_ips)?,
            deny_ips: serde_json::to_value(deny_ips)?,
            created_by: user_id,
            // We use and_then here to coalesce the scheduler being None and the similarity being
            // None. This is because both the scheduler and the similarity are optional.
            scheduler_similarity: scheduler.and_then(|s| s.similarity().into_model()),
            // Here we use `map` and `transpose`, because the scheduler is optional, but if it is
            // provided, the `resource` is not optional.
            scheduler_resource: scheduler.map(|s| s.resource().into_model()).transpose()?,
        })
    }

    fn host_id(&self) -> crate::Result<Option<HostId>> {
        let placement = self
            .placement
            .as_ref()
            .ok_or_else(helpers::required("placement"))?
            .placement
            .as_ref()
            .ok_or_else(helpers::required("placement"))?;
        match placement {
            api::node_placement::Placement::Scheduler(_) => Ok(None),
            api::node_placement::Placement::HostId(id) => Ok(Some(id.parse()?)),
        }
    }

    fn properties(
        &self,
        node: &Node,
        name_to_id_map: HashMap<String, uuid::Uuid>,
    ) -> crate::Result<Vec<NodeProperty>> {
        self.properties
            .iter()
            .map(|prop| {
                let err = || crate::Error::unexpected(format!("No prop named {} found", prop.name));
                Ok(NodeProperty {
                    id: uuid::Uuid::new_v4(),
                    node_id: node.id,
                    blockchain_property_id: name_to_id_map
                        .get(&prop.name)
                        .copied()
                        .ok_or_else(err)?,
                    value: prop.value.clone(),
                })
            })
            .collect()
    }
}

impl api::NodeServiceListRequest {
    fn as_filter(&self) -> crate::Result<NodeFilter> {
        Ok(NodeFilter {
            org_id: self.org_id.parse()?,
            offset: self.offset,
            limit: self.limit,
            status: self.statuses().map(|s| s.into_model()).collect(),
            node_types: self.node_types().map(|t| t.into_model()).collect(),
            blockchains: self
                .blockchain_ids
                .iter()
                .map(|id| id.parse())
                .collect::<Result<_, _>>()?,
            host_id: self.host_id.as_ref().map(|id| id.parse()).transpose()?,
        })
    }
}

impl api::NodeServiceUpdateConfigRequest {
    pub fn as_update(&self) -> crate::Result<UpdateNode<'_>> {
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
            id: self.id.parse()?,
            name: None,
            version: None,
            ip_addr: None,
            block_height: None,
            node_data: None,
            chain_status: None,
            sync_status: None,
            staking_status: None,
            container_status: None,
            self_update: self.self_update,
            address: None,
            allow_ips: Some(serde_json::to_value(allow_ips)?),
            deny_ips: Some(serde_json::to_value(deny_ips)?),
        })
    }
}

impl api::NodeServiceUpdateStatusRequest {
    pub fn as_update(&self) -> crate::Result<UpdateNode<'_>> {
        Ok(UpdateNode {
            id: self.id.parse()?,
            name: None,
            version: self.version.as_deref(),
            ip_addr: None,
            block_height: None,
            node_data: None,
            chain_status: None,
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

impl api::NodeType {
    pub fn from_model(model: NodeType) -> Self {
        match model {
            NodeType::Unknown => Self::Unspecified,
            NodeType::Miner => Self::Miner,
            NodeType::Etl => Self::Etl,
            NodeType::Validator => Self::Validator,
            NodeType::Api => Self::Api,
            NodeType::Oracle => Self::Oracle,
            NodeType::Relay => Self::Relay,
            NodeType::Execution => Self::Execution,
            NodeType::Beacon => Self::Beacon,
            NodeType::MevBoost => Self::Mevboost,
            NodeType::Node => Self::Node,
            NodeType::FullNode => Self::Fullnode,
            NodeType::LightNode => Self::Lightnode,
        }
    }

    fn into_model(self) -> NodeType {
        match self {
            Self::Unspecified => NodeType::Unknown,
            Self::Miner => NodeType::Miner,
            Self::Etl => NodeType::Etl,
            Self::Validator => NodeType::Validator,
            Self::Api => NodeType::Api,
            Self::Oracle => NodeType::Oracle,
            Self::Relay => NodeType::Relay,
            Self::Execution => NodeType::Execution,
            Self::Beacon => NodeType::Beacon,
            Self::Mevboost => NodeType::MevBoost,
            Self::Node => NodeType::Node,
            Self::Fullnode => NodeType::FullNode,
            Self::Lightnode => NodeType::LightNode,
        }
    }
}

impl api::ContainerStatus {
    fn from_model(model: ContainerStatus) -> Self {
        match model {
            ContainerStatus::Unknown => Self::Unspecified,
            ContainerStatus::Creating => Self::Creating,
            ContainerStatus::Running => Self::Running,
            ContainerStatus::Starting => Self::Starting,
            ContainerStatus::Stopping => Self::Stopping,
            ContainerStatus::Stopped => Self::Stopped,
            ContainerStatus::Upgrading => Self::Upgrading,
            ContainerStatus::Upgraded => Self::Upgraded,
            ContainerStatus::Deleting => Self::Deleting,
            ContainerStatus::Deleted => Self::Deleted,
            ContainerStatus::Installing => Self::Installing,
            ContainerStatus::Snapshotting => Self::Snapshotting,
            ContainerStatus::Failed => Self::Failed,
        }
    }

    fn into_model(self) -> ContainerStatus {
        match self {
            Self::Unspecified => ContainerStatus::Unknown,
            Self::Creating => ContainerStatus::Creating,
            Self::Running => ContainerStatus::Running,
            Self::Starting => ContainerStatus::Starting,
            Self::Stopping => ContainerStatus::Stopping,
            Self::Stopped => ContainerStatus::Stopped,
            Self::Upgrading => ContainerStatus::Upgrading,
            Self::Upgraded => ContainerStatus::Upgraded,
            Self::Deleting => ContainerStatus::Deleting,
            Self::Deleted => ContainerStatus::Deleted,
            Self::Installing => ContainerStatus::Installing,
            Self::Snapshotting => ContainerStatus::Snapshotting,
            Self::Failed => ContainerStatus::Failed,
        }
    }
}

impl api::NodeStatus {
    fn from_model(model: NodeChainStatus) -> Self {
        match model {
            NodeChainStatus::Unknown => Self::Unspecified,
            NodeChainStatus::Provisioning => Self::Provisioning,
            NodeChainStatus::Broadcasting => Self::Broadcasting,
            NodeChainStatus::Cancelled => Self::Cancelled,
            NodeChainStatus::Delegating => Self::Delegating,
            NodeChainStatus::Delinquent => Self::Delinquent,
            NodeChainStatus::Disabled => Self::Disabled,
            NodeChainStatus::Earning => Self::Earning,
            NodeChainStatus::Electing => Self::Electing,
            NodeChainStatus::Elected => Self::Elected,
            NodeChainStatus::Exported => Self::Exported,
            NodeChainStatus::Ingesting => Self::Ingesting,
            NodeChainStatus::Mining => Self::Mining,
            NodeChainStatus::Minting => Self::Minting,
            NodeChainStatus::Processing => Self::Processing,
            NodeChainStatus::Relaying => Self::Relaying,
            NodeChainStatus::Removed => Self::Removed,
            NodeChainStatus::Removing => Self::Removing,
        }
    }

    pub fn into_model(self) -> NodeChainStatus {
        match self {
            Self::Unspecified => NodeChainStatus::Unknown,
            Self::Provisioning => NodeChainStatus::Provisioning,
            Self::Broadcasting => NodeChainStatus::Broadcasting,
            Self::Cancelled => NodeChainStatus::Cancelled,
            Self::Delegating => NodeChainStatus::Delegating,
            Self::Delinquent => NodeChainStatus::Delinquent,
            Self::Disabled => NodeChainStatus::Disabled,
            Self::Earning => NodeChainStatus::Earning,
            Self::Electing => NodeChainStatus::Electing,
            Self::Elected => NodeChainStatus::Elected,
            Self::Exported => NodeChainStatus::Exported,
            Self::Ingesting => NodeChainStatus::Ingesting,
            Self::Mining => NodeChainStatus::Mining,
            Self::Minting => NodeChainStatus::Minting,
            Self::Processing => NodeChainStatus::Processing,
            Self::Relaying => NodeChainStatus::Relaying,
            Self::Removed => NodeChainStatus::Removed,
            Self::Removing => NodeChainStatus::Removing,
        }
    }
}

impl api::StakingStatus {
    fn from_model(model: NodeStakingStatus) -> Self {
        match model {
            NodeStakingStatus::Unknown => Self::Unspecified,
            NodeStakingStatus::Follower => Self::Follower,
            NodeStakingStatus::Staked => Self::Staked,
            NodeStakingStatus::Staking => Self::Staking,
            NodeStakingStatus::Validating => Self::Validating,
            NodeStakingStatus::Consensus => Self::Consensus,
            NodeStakingStatus::Unstaked => Self::Unstaked,
        }
    }

    pub fn into_model(self) -> NodeStakingStatus {
        match self {
            Self::Unspecified => NodeStakingStatus::Unknown,
            Self::Follower => NodeStakingStatus::Follower,
            Self::Staked => NodeStakingStatus::Staked,
            Self::Staking => NodeStakingStatus::Staking,
            Self::Validating => NodeStakingStatus::Validating,
            Self::Consensus => NodeStakingStatus::Consensus,
            Self::Unstaked => NodeStakingStatus::Unstaked,
        }
    }
}

impl api::SyncStatus {
    fn from_model(model: NodeSyncStatus) -> Self {
        match model {
            NodeSyncStatus::Unknown => Self::Unspecified,
            NodeSyncStatus::Syncing => Self::Syncing,
            NodeSyncStatus::Synced => Self::Synced,
        }
    }

    pub fn into_model(self) -> NodeSyncStatus {
        match self {
            Self::Unspecified => NodeSyncStatus::Unknown,
            Self::Syncing => NodeSyncStatus::Syncing,
            Self::Synced => NodeSyncStatus::Synced,
        }
    }
}

impl api::NodeProperty {
    fn from_model(model: NodeProperty, bprop: BlockchainProperty) -> Self {
        let mut prop = Self {
            name: bprop.name,
            ui_type: 0,
            disabled: bprop.disabled,
            required: bprop.required,
            value: model.value,
        };
        prop.set_ui_type(api::UiType::from_model(bprop.ui_type));
        prop
    }
}

impl api::UiType {
    pub fn from_model(model: BlockchainPropertyUiType) -> Self {
        match model {
            BlockchainPropertyUiType::Switch => api::UiType::Switch,
            BlockchainPropertyUiType::Password => api::UiType::Password,
            BlockchainPropertyUiType::Text => api::UiType::Text,
            BlockchainPropertyUiType::FileUpload => api::UiType::FileUpload,
        }
    }

    pub fn into_model(self) -> crate::Result<BlockchainPropertyUiType> {
        match self {
            Self::Unspecified => Err(anyhow::anyhow!("UiType not specified!").into()),
            Self::Switch => Ok(BlockchainPropertyUiType::Switch),
            Self::Password => Ok(BlockchainPropertyUiType::Password),
            Self::Text => Ok(BlockchainPropertyUiType::Text),
            Self::FileUpload => Ok(BlockchainPropertyUiType::FileUpload),
        }
    }
}

impl api::NodeScheduler {
    fn new(node: NodeScheduler) -> Self {
        use api::node_scheduler::{ResourceAffinity, SimilarNodeAffinity};

        let mut scheduler = Self {
            similarity: None,
            resource: 0,
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

pub(super) async fn create_node_command(
    node: &Node,
    cmd_type: CommandType,
    conn: &mut Conn<'_>,
) -> crate::Result<Command> {
    let new_command = NewCommand {
        host_id: node.host_id,
        cmd: cmd_type,
        sub_cmd: None,
        node_id: Some(node.id),
    };
    new_command.create(conn).await
}

impl api::node_scheduler::SimilarNodeAffinity {
    fn from_model(model: SimilarNodeAffinity) -> Self {
        match model {
            SimilarNodeAffinity::Cluster => Self::Cluster,
            SimilarNodeAffinity::Spread => Self::Spread,
        }
    }

    fn into_model(self) -> Option<SimilarNodeAffinity> {
        match self {
            Self::Unspecified => None,
            Self::Cluster => Some(SimilarNodeAffinity::Cluster),
            Self::Spread => Some(SimilarNodeAffinity::Spread),
        }
    }
}

impl api::node_scheduler::ResourceAffinity {
    fn from_model(model: ResourceAffinity) -> Self {
        match model {
            ResourceAffinity::MostResources => Self::MostResources,
            ResourceAffinity::LeastResources => Self::LeastResources,
        }
    }

    fn into_model(self) -> crate::Result<ResourceAffinity> {
        match self {
            Self::Unspecified => Err(anyhow::anyhow!("Unspecified resource affinity").into()),
            Self::MostResources => Ok(ResourceAffinity::MostResources),
            Self::LeastResources => Ok(ResourceAffinity::LeastResources),
        }
    }
}
