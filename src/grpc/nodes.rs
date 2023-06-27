use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;
use futures_util::future::OptionFuture;

use super::api::{self, node_service_server};
use super::helpers;
use crate::auth::token::{Endpoint, Resource};
use crate::cookbook::script::HardwareRequirements;
use crate::{auth, models};

struct NodeCommandResult<T> {
    commands: Vec<api::Command>,
    node_message: api::NodeMessage,
    resp: tonic::Response<T>,
}

struct NodeResult<T> {
    node_message: api::NodeMessage,
    resp: tonic::Response<T>,
}

#[tonic::async_trait]
impl node_service_server::NodeService for super::GrpcImpl {
    async fn create(
        &self,
        req: tonic::Request<api::NodeServiceCreateRequest>,
    ) -> super::Resp<api::NodeServiceCreateResponse> {
        let result = self.trx(|c| create(self, req, c).scope_boxed()).await?;
        for command in result.commands {
            self.notifier.commands_sender().send(&command).await?;
        }
        self.notifier
            .nodes_sender()
            .send(&result.node_message)
            .await?;
        Ok(result.resp)
    }

    async fn get(
        &self,
        req: tonic::Request<api::NodeServiceGetRequest>,
    ) -> super::Resp<api::NodeServiceGetResponse> {
        let mut conn = self.conn().await?;
        let resp = get(req, &mut conn).await?;
        Ok(resp)
    }

    async fn list(
        &self,
        req: tonic::Request<api::NodeServiceListRequest>,
    ) -> super::Resp<api::NodeServiceListResponse> {
        let mut conn = self.conn().await?;
        let resp = list(req, &mut conn).await?;
        Ok(resp)
    }

    async fn update(
        &self,
        req: tonic::Request<api::NodeServiceUpdateRequest>,
    ) -> super::Resp<api::NodeServiceUpdateResponse> {
        let result = self.trx(|c| update(req, c).scope_boxed()).await?;
        self.notifier
            .nodes_sender()
            .send(&result.node_message)
            .await?;
        Ok(result.resp)
    }

    async fn delete(
        &self,
        req: tonic::Request<api::NodeServiceDeleteRequest>,
    ) -> super::Resp<api::NodeServiceDeleteResponse> {
        let result = self.trx(|c| delete(self, req, c).scope_boxed()).await?;
        for command in result.commands {
            self.notifier.commands_sender().send(&command).await?;
        }
        self.notifier
            .nodes_sender()
            .send(&result.node_message)
            .await?;
        Ok(result.resp)
    }
}

async fn get(
    req: tonic::Request<api::NodeServiceGetRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::NodeServiceGetResponse> {
    let claims = auth::get_claims(&req, Endpoint::NodeCreate, conn).await?;
    let req = req.into_inner();
    let node = models::Node::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, node.org_id, conn).await?,
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
    conn: &mut models::Conn,
) -> super::Result<api::NodeServiceListResponse> {
    let claims = auth::get_claims(&req, Endpoint::NodeList, conn).await?;
    let filter = req.into_inner().as_filter()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, filter.org_id, conn).await?,
        Resource::Org(org_id) => filter.org_id == org_id,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let nodes = models::Node::filter(filter, conn).await?;
    let nodes = api::Node::from_models(nodes, conn).await?;
    let resp = api::NodeServiceListResponse { nodes };
    Ok(tonic::Response::new(resp))
}

async fn create(
    grpc: &super::GrpcImpl,
    req: tonic::Request<api::NodeServiceCreateRequest>,
    conn: &mut models::Conn,
) -> crate::Result<NodeCommandResult<api::NodeServiceCreateResponse>> {
    let claims = auth::get_claims(&req, Endpoint::NodeCreate, conn).await?;
    let Resource::User(user_id) = claims.resource() else { super::forbidden!("Need user_id!") };

    let user = models::User::find_by_id(user_id, conn).await?;
    let req = req.into_inner();
    let blockchain = models::Blockchain::find_by_id(req.blockchain_id.parse()?, conn).await?;
    // We want to cast a string like `NODE_TYPE_VALIDATOR` to `validator`.
    let node_type = req.node_type().as_str_name()[10..].to_lowercase();
    let reqs = grpc
        .cookbook
        .rhai_metadata(&blockchain.name, &node_type, &req.version)
        .await?
        .requirements;
    let new_node = req.as_new(user.id, reqs)?;
    // The host_id will either be determined by the scheduler, or by the host_id.
    // Therfore we pass in an optional host_id for the node creation to fall back on if
    // there is no scheduler.
    let host_id = req.host_id()?;
    let host = if let Some(host_id) = host_id {
        let host = models::Host::find_by_id(host_id, conn).await?;
        let Some(org_id) = host.org_id else { super::forbidden!("Host must have org_id") };
        if !models::Org::is_member(user.id, org_id, conn).await? {
            super::forbidden!("Must be member of org");
        }
        Some(host)
    } else {
        None
    };
    let node = new_node
        .create(host, &grpc.dns, &grpc.cookbook, conn)
        .await?;
    // The user sends in the properties in a key-value style, that is,
    // { property name: property value }. We want to store this as
    // { property id: property value }. In order to map property names to property ids we can use
    // the id to name map, and then flip the keys and values to create an id to name map. Note that
    // this requires the names to be unique, but we expect this to be the case.
    let name_to_id_map = models::BlockchainProperty::id_to_name_map(
        &blockchain,
        node.node_type,
        &node.version,
        conn,
    )
    .await?
    .into_iter()
    .map(|(k, v)| (v, k))
    .collect();
    models::NodeProperty::bulk_create(req.properties(&node, name_to_id_map)?, conn).await?;
    let mut vec_commands = Vec::new();
    let create_notif = create_create_node_command(&node, conn).await?;
    let create_cmd = api::Command::from_model(&create_notif, conn).await?;
    vec_commands.push(create_cmd);
    let start_notif = create_restart_node_command(&node, conn).await?;
    let start_cmd = api::Command::from_model(&start_notif, conn).await?;
    vec_commands.push(start_cmd);
    let node_api = api::Node::from_model(node, conn).await?;
    let created = api::NodeMessage::created(node_api.clone(), user.clone());
    let resp = api::NodeServiceCreateResponse {
        node: Some(node_api),
    };

    Ok(NodeCommandResult {
        resp: tonic::Response::new(resp),
        commands: vec_commands,
        node_message: created,
    })
}

async fn update(
    req: tonic::Request<api::NodeServiceUpdateRequest>,
    conn: &mut models::Conn,
) -> crate::Result<NodeResult<api::NodeServiceUpdateResponse>> {
    let claims = auth::get_claims(&req, Endpoint::NodeUpdate, conn).await?;
    let req = req.into_inner();
    let node = models::Node::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, node.org_id, conn).await?,
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
        .map_user(|id| models::User::find_by_id(id, conn));
    let user = OptionFuture::from(user).await.transpose()?;
    let node = update_node.update(conn).await?;
    let msg = api::NodeMessage::updated(node, user, conn).await?;
    let resp = api::NodeServiceUpdateResponse {};
    Ok(NodeResult {
        resp: tonic::Response::new(resp),
        node_message: msg,
    })
}

async fn delete(
    grpc: &super::GrpcImpl,
    req: tonic::Request<api::NodeServiceDeleteRequest>,
    conn: &mut models::Conn,
) -> crate::Result<NodeCommandResult<api::NodeServiceDeleteResponse>> {
    let claims = auth::get_claims(&req, Endpoint::NodeDelete, conn).await?;
    let Resource::User(user_id) = claims.resource() else { super::forbidden!("Need user_id!") };
    let req = req.into_inner();
    let node = models::Node::find_by_id(req.id.parse()?, conn).await?;

    if !models::Org::is_member(user_id, node.org_id, conn).await? {
        super::forbidden!("User cannot delete node");
    }
    // 1. Delete node, if the node belongs to the current user
    // Key files are deleted automatically because of 'on delete cascade' in tables DDL
    models::Node::delete(node.id, &grpc.dns, conn).await?;

    let host_id = node.host_id;
    // 2. Do NOT delete reserved IP addresses, but set assigned to false
    let ip_addr = node.ip_addr.parse()?;
    let ip = models::IpAddress::find_by_node(ip_addr, conn).await?;

    models::IpAddress::unassign(ip.id, host_id, conn).await?;

    // Delete all pending commands for this node: there are not useable anymore
    models::Command::delete_pending(node.id, conn).await?;

    // Send delete node command
    let node_id = node.id.to_string();
    let new_command = models::NewCommand {
        host_id: node.host_id,
        cmd: models::CommandType::DeleteNode,
        sub_cmd: Some(&node_id),
        // Note that the `node_id` goes into the `sub_cmd` field, not the node_id
        // field, because the node was just deleted.
        node_id: None,
    };
    let cmd = new_command.create(conn).await?;

    let user = models::User::find_by_id(user_id, conn).await?;

    let cmd = api::Command::from_model(&cmd, conn).await?;

    let deleted = api::NodeMessage::deleted(node, user);
    let resp = api::NodeServiceDeleteResponse {};
    Ok(NodeCommandResult {
        resp: tonic::Response::new(resp),
        commands: vec![cmd],
        node_message: deleted,
    })
}

impl api::Node {
    /// This function is used to create a ui node from a database node. We want to include the
    /// `database_name` in the ui representation, but it is not in the node model. Therefore we
    /// perform a seperate query to the blockchains table.
    pub async fn from_model(node: models::Node, conn: &mut models::Conn) -> crate::Result<Self> {
        let blockchain = models::Blockchain::find_by_id(node.blockchain_id, conn).await?;
        let user_fut = node
            .created_by
            .map(|u_id| models::User::find_by_id(u_id, conn));
        let user = OptionFuture::from(user_fut).await.transpose()?;

        // We need to get both the node properties and the blockchain properties to construct the
        // final dto. First we query both, and then we zip them together.
        let nprops = models::NodeProperty::by_node(&node, conn).await?;
        let bprops = models::BlockchainProperty::by_node_props(&nprops, conn).await?;
        let bprops: HashMap<_, _> = bprops.into_iter().map(|prop| (prop.id, prop)).collect();
        let props = nprops
            .into_iter()
            .map(|nprop| (nprop.blockchain_property_id, nprop))
            .map(|(blockchain_property_id, nprop)| (nprop, bprops[&blockchain_property_id].clone()))
            .collect();

        Self::new(node, &blockchain, user.as_ref(), props)
    }

    /// This function is used to create many ui nodes from many database nodes. The same
    /// justification as above applies. Note that this function does not simply defer to the
    /// function above, but rather it performs 1 query for n nodes. We like it this way :)
    pub async fn from_models(
        nodes: Vec<models::Node>,
        conn: &mut models::Conn,
    ) -> crate::Result<Vec<Self>> {
        let blockchain_ids: Vec<_> = nodes.iter().map(|n| n.blockchain_id).collect();
        let blockchains: HashMap<_, _> = models::Blockchain::find_by_ids(&blockchain_ids, conn)
            .await?
            .into_iter()
            .map(|b| (b.id, b))
            .collect();
        let user_ids: Vec<_> = nodes.iter().flat_map(|n| n.created_by).collect();
        let users: HashMap<_, _> = models::User::find_by_ids(&user_ids, conn)
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();

        let nprops = models::NodeProperty::by_nodes(&nodes, conn).await?;
        let bprops = models::BlockchainProperty::by_node_props(&nprops, conn).await?;
        let bprops: HashMap<_, _> = bprops.into_iter().map(|prop| (prop.id, prop)).collect();
        let mut props_map: HashMap<uuid::Uuid, Vec<(_, _)>> = HashMap::new();
        for nprop in nprops {
            let blockchain_property_id = nprop.blockchain_property_id;
            props_map
                .entry(nprop.node_id)
                .or_default()
                .push((nprop, bprops[&blockchain_property_id].clone()));
        }

        nodes
            .into_iter()
            .map(|n| (n.id, n.blockchain_id, n.created_by, n))
            .map(|(n_id, b_id, u_id, n)| {
                Self::new(
                    n,
                    &blockchains[&b_id],
                    u_id.and_then(|u_id| users.get(&u_id)),
                    props_map[&n_id].clone(),
                )
            })
            .collect()
    }

    /// Construct a new ui node from the queried parts.
    fn new(
        node: models::Node,
        blockchain: &models::Blockchain,
        user: Option<&models::User>,
        properties: Vec<(models::NodeProperty, models::BlockchainProperty)>,
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
            ip: Some(node.ip_addr),
            ip_gateway: node.ip_gateway,
            node_type: 0, // We use the setter to set this field for type-safety
            properties,
            block_height: node.block_height.map(u64::try_from).transpose()?,
            created_at: Some(super::try_dt_to_ts(node.created_at)?),
            updated_at: Some(super::try_dt_to_ts(node.updated_at)?),
            status: 0,            // We use the setter to set this field for type-safety
            staking_status: None, // We use the setter to set this field for type-safety
            container_status: 0,  // We use the setter to set this field for type-safety
            sync_status: 0,       // We use the setter to set this field for type-safety
            self_update: node.self_update,
            network: node.network,
            blockchain_name: Some(blockchain.name.clone()),
            created_by: user.map(|u| u.id.to_string()),
            created_by_name: user.map(|u| format!("{} {}", u.first_name, u.last_name)),
            created_by_email: user.map(|u| u.email.clone()),
            allow_ips,
            deny_ips,
            placement: Some(placement),
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
    pub fn as_new(
        &self,
        user_id: uuid::Uuid,
        req: HardwareRequirements,
    ) -> crate::Result<models::NewNode<'_>> {
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
        let allow_ips: Vec<models::FilteredIpAddr> = self
            .allow_ips
            .iter()
            .map(api::FilteredIpAddr::as_model)
            .collect();
        let deny_ips: Vec<models::FilteredIpAddr> = self
            .deny_ips
            .iter()
            .map(api::FilteredIpAddr::as_model)
            .collect();
        Ok(models::NewNode {
            id: uuid::Uuid::new_v4(),
            org_id: self.org_id.parse()?,
            name: petname::Petnames::large().generate_one(3, "_"),
            version: &self.version,
            blockchain_id: self.blockchain_id.parse()?,
            block_height: None,
            node_data: None,
            chain_status: models::NodeChainStatus::Provisioning,
            sync_status: models::NodeSyncStatus::Unknown,
            staking_status: models::NodeStakingStatus::Unknown,
            container_status: models::ContainerStatus::Unknown,
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

    fn host_id(&self) -> crate::Result<Option<uuid::Uuid>> {
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
        node: &models::Node,
        name_to_id_map: HashMap<String, uuid::Uuid>,
    ) -> crate::Result<Vec<models::NodeProperty>> {
        self.properties
            .iter()
            .map(|prop| {
                let err = || crate::Error::unexpected(format!("No prop named {} found", prop.name));
                Ok(models::NodeProperty {
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
    fn as_filter(&self) -> crate::Result<models::NodeFilter> {
        Ok(models::NodeFilter {
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

impl api::NodeServiceUpdateRequest {
    pub fn as_update(&self) -> crate::Result<models::UpdateNode> {
        // Convert the ip list from the gRPC structures to the database models.
        let allow_ips: Vec<models::FilteredIpAddr> = self
            .allow_ips
            .iter()
            .map(api::FilteredIpAddr::as_model)
            .collect();
        let deny_ips: Vec<models::FilteredIpAddr> = self
            .deny_ips
            .iter()
            .map(api::FilteredIpAddr::as_model)
            .collect();

        Ok(models::UpdateNode {
            id: self.id.parse()?,
            name: None,
            version: None,
            ip_addr: None,
            block_height: None,
            node_data: None,
            chain_status: None,
            sync_status: None,
            staking_status: None,
            container_status: Some(self.container_status().into_model()),
            self_update: self.self_update,
            address: self.address.as_deref(),
            allow_ips: Some(serde_json::to_value(allow_ips)?),
            deny_ips: Some(serde_json::to_value(deny_ips)?),
        })
    }
}

impl api::NodeType {
    pub fn from_model(model: models::NodeType) -> Self {
        match model {
            models::NodeType::Unknown => Self::Unspecified,
            models::NodeType::Miner => Self::Miner,
            models::NodeType::Etl => Self::Etl,
            models::NodeType::Validator => Self::Validator,
            models::NodeType::Api => Self::Api,
            models::NodeType::Oracle => Self::Oracle,
            models::NodeType::Relay => Self::Relay,
            models::NodeType::Execution => Self::Execution,
            models::NodeType::Beacon => Self::Beacon,
            models::NodeType::MevBoost => Self::Mevboost,
            models::NodeType::Node => Self::Node,
            models::NodeType::FullNode => Self::Fullnode,
            models::NodeType::LightNode => Self::Lightnode,
        }
    }

    fn into_model(self) -> models::NodeType {
        match self {
            Self::Unspecified => models::NodeType::Unknown,
            Self::Miner => models::NodeType::Miner,
            Self::Etl => models::NodeType::Etl,
            Self::Validator => models::NodeType::Validator,
            Self::Api => models::NodeType::Api,
            Self::Oracle => models::NodeType::Oracle,
            Self::Relay => models::NodeType::Relay,
            Self::Execution => models::NodeType::Execution,
            Self::Beacon => models::NodeType::Beacon,
            Self::Mevboost => models::NodeType::MevBoost,
            Self::Node => models::NodeType::Node,
            Self::Fullnode => models::NodeType::FullNode,
            Self::Lightnode => models::NodeType::LightNode,
        }
    }
}

impl api::ContainerStatus {
    fn from_model(model: models::ContainerStatus) -> Self {
        match model {
            models::ContainerStatus::Unknown => Self::Unspecified,
            models::ContainerStatus::Creating => Self::Creating,
            models::ContainerStatus::Running => Self::Running,
            models::ContainerStatus::Starting => Self::Starting,
            models::ContainerStatus::Stopping => Self::Stopping,
            models::ContainerStatus::Stopped => Self::Stopped,
            models::ContainerStatus::Upgrading => Self::Upgrading,
            models::ContainerStatus::Upgraded => Self::Upgraded,
            models::ContainerStatus::Deleting => Self::Deleting,
            models::ContainerStatus::Deleted => Self::Deleted,
            models::ContainerStatus::Installing => Self::Installing,
            models::ContainerStatus::Snapshotting => Self::Snapshotting,
            models::ContainerStatus::Failed => Self::Failed,
        }
    }

    fn into_model(self) -> models::ContainerStatus {
        match self {
            Self::Unspecified => models::ContainerStatus::Unknown,
            Self::Creating => models::ContainerStatus::Creating,
            Self::Running => models::ContainerStatus::Running,
            Self::Starting => models::ContainerStatus::Starting,
            Self::Stopping => models::ContainerStatus::Stopping,
            Self::Stopped => models::ContainerStatus::Stopped,
            Self::Upgrading => models::ContainerStatus::Upgrading,
            Self::Upgraded => models::ContainerStatus::Upgraded,
            Self::Deleting => models::ContainerStatus::Deleting,
            Self::Deleted => models::ContainerStatus::Deleted,
            Self::Installing => models::ContainerStatus::Installing,
            Self::Snapshotting => models::ContainerStatus::Snapshotting,
            Self::Failed => models::ContainerStatus::Failed,
        }
    }
}

impl api::NodeStatus {
    fn from_model(model: models::NodeChainStatus) -> Self {
        match model {
            models::NodeChainStatus::Unknown => Self::Unspecified,
            models::NodeChainStatus::Provisioning => Self::Provisioning,
            models::NodeChainStatus::Broadcasting => Self::Broadcasting,
            models::NodeChainStatus::Cancelled => Self::Cancelled,
            models::NodeChainStatus::Delegating => Self::Delegating,
            models::NodeChainStatus::Delinquent => Self::Delinquent,
            models::NodeChainStatus::Disabled => Self::Disabled,
            models::NodeChainStatus::Earning => Self::Earning,
            models::NodeChainStatus::Electing => Self::Electing,
            models::NodeChainStatus::Elected => Self::Elected,
            models::NodeChainStatus::Exported => Self::Exported,
            models::NodeChainStatus::Ingesting => Self::Ingesting,
            models::NodeChainStatus::Mining => Self::Mining,
            models::NodeChainStatus::Minting => Self::Minting,
            models::NodeChainStatus::Processing => Self::Processing,
            models::NodeChainStatus::Relaying => Self::Relaying,
            models::NodeChainStatus::Removed => Self::Removed,
            models::NodeChainStatus::Removing => Self::Removing,
        }
    }

    pub fn into_model(self) -> models::NodeChainStatus {
        match self {
            Self::Unspecified => models::NodeChainStatus::Unknown,
            Self::Provisioning => models::NodeChainStatus::Provisioning,
            Self::Broadcasting => models::NodeChainStatus::Broadcasting,
            Self::Cancelled => models::NodeChainStatus::Cancelled,
            Self::Delegating => models::NodeChainStatus::Delegating,
            Self::Delinquent => models::NodeChainStatus::Delinquent,
            Self::Disabled => models::NodeChainStatus::Disabled,
            Self::Earning => models::NodeChainStatus::Earning,
            Self::Electing => models::NodeChainStatus::Electing,
            Self::Elected => models::NodeChainStatus::Elected,
            Self::Exported => models::NodeChainStatus::Exported,
            Self::Ingesting => models::NodeChainStatus::Ingesting,
            Self::Mining => models::NodeChainStatus::Mining,
            Self::Minting => models::NodeChainStatus::Minting,
            Self::Processing => models::NodeChainStatus::Processing,
            Self::Relaying => models::NodeChainStatus::Relaying,
            Self::Removed => models::NodeChainStatus::Removed,
            Self::Removing => models::NodeChainStatus::Removing,
        }
    }
}

impl api::StakingStatus {
    fn from_model(model: models::NodeStakingStatus) -> Self {
        match model {
            models::NodeStakingStatus::Unknown => Self::Unspecified,
            models::NodeStakingStatus::Follower => Self::Follower,
            models::NodeStakingStatus::Staked => Self::Staked,
            models::NodeStakingStatus::Staking => Self::Staking,
            models::NodeStakingStatus::Validating => Self::Validating,
            models::NodeStakingStatus::Consensus => Self::Consensus,
            models::NodeStakingStatus::Unstaked => Self::Unstaked,
        }
    }

    pub fn into_model(self) -> models::NodeStakingStatus {
        match self {
            Self::Unspecified => models::NodeStakingStatus::Unknown,
            Self::Follower => models::NodeStakingStatus::Follower,
            Self::Staked => models::NodeStakingStatus::Staked,
            Self::Staking => models::NodeStakingStatus::Staking,
            Self::Validating => models::NodeStakingStatus::Validating,
            Self::Consensus => models::NodeStakingStatus::Consensus,
            Self::Unstaked => models::NodeStakingStatus::Unstaked,
        }
    }
}

impl api::SyncStatus {
    fn from_model(model: models::NodeSyncStatus) -> Self {
        match model {
            models::NodeSyncStatus::Unknown => Self::Unspecified,
            models::NodeSyncStatus::Syncing => Self::Syncing,
            models::NodeSyncStatus::Synced => Self::Synced,
        }
    }

    pub fn into_model(self) -> models::NodeSyncStatus {
        match self {
            Self::Unspecified => models::NodeSyncStatus::Unknown,
            Self::Syncing => models::NodeSyncStatus::Syncing,
            Self::Synced => models::NodeSyncStatus::Synced,
        }
    }
}

impl api::NodeProperty {
    fn from_model(model: models::NodeProperty, bprop: models::BlockchainProperty) -> Self {
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
    pub fn from_model(model: models::BlockchainPropertyUiType) -> Self {
        match model {
            models::BlockchainPropertyUiType::Switch => api::UiType::Switch,
            models::BlockchainPropertyUiType::Password => api::UiType::Password,
            models::BlockchainPropertyUiType::Text => api::UiType::Text,
            models::BlockchainPropertyUiType::FileUpload => api::UiType::FileUpload,
        }
    }

    pub fn into_model(self) -> crate::Result<models::BlockchainPropertyUiType> {
        match self {
            Self::Unspecified => Err(anyhow::anyhow!("UiType not specified!").into()),
            Self::Switch => Ok(models::BlockchainPropertyUiType::Switch),
            Self::Password => Ok(models::BlockchainPropertyUiType::Password),
            Self::Text => Ok(models::BlockchainPropertyUiType::Text),
            Self::FileUpload => Ok(models::BlockchainPropertyUiType::FileUpload),
        }
    }
}

impl api::NodeScheduler {
    fn new(node: models::NodeScheduler) -> Self {
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
    fn from_model(model: models::FilteredIpAddr) -> Self {
        Self {
            ip: model.ip,
            description: model.description,
        }
    }

    fn as_model(&self) -> models::FilteredIpAddr {
        models::FilteredIpAddr {
            ip: self.ip.clone(),
            description: self.description.clone(),
        }
    }
}

pub(super) async fn create_create_node_command(
    node: &models::Node,
    conn: &mut models::Conn,
) -> crate::Result<models::Command> {
    let new_command = models::NewCommand {
        host_id: node.host_id,
        cmd: models::CommandType::CreateNode,
        sub_cmd: None,
        node_id: Some(node.id),
    };
    new_command.create(conn).await
}

pub(super) async fn create_restart_node_command(
    node: &models::Node,
    conn: &mut models::Conn,
) -> crate::Result<models::Command> {
    let new_command = models::NewCommand {
        host_id: node.host_id,
        cmd: models::CommandType::RestartNode,
        sub_cmd: None,
        node_id: Some(node.id),
    };
    new_command.create(conn).await
}

impl api::node_scheduler::SimilarNodeAffinity {
    fn from_model(model: models::SimilarNodeAffinity) -> Self {
        match model {
            models::SimilarNodeAffinity::Cluster => Self::Cluster,
            models::SimilarNodeAffinity::Spread => Self::Spread,
        }
    }

    fn into_model(self) -> Option<models::SimilarNodeAffinity> {
        match self {
            Self::Unspecified => None,
            Self::Cluster => Some(models::SimilarNodeAffinity::Cluster),
            Self::Spread => Some(models::SimilarNodeAffinity::Spread),
        }
    }
}

impl api::node_scheduler::ResourceAffinity {
    fn from_model(model: models::ResourceAffinity) -> Self {
        match model {
            models::ResourceAffinity::MostResources => Self::MostResources,
            models::ResourceAffinity::LeastResources => Self::LeastResources,
        }
    }

    fn into_model(self) -> crate::Result<models::ResourceAffinity> {
        match self {
            Self::Unspecified => Err(anyhow::anyhow!("Unspecified resource affinity").into()),
            Self::MostResources => Ok(models::ResourceAffinity::MostResources),
            Self::LeastResources => Ok(models::ResourceAffinity::LeastResources),
        }
    }
}
