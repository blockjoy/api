use std::collections::HashSet;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::{CryptPerm, NodeAdminPerm, NodePerm, Perm};
use crate::auth::resource::{NodeId, OrgId, Resource};
use crate::auth::{AuthZ, Authorize};
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::model::command::NewCommand;
use crate::model::image::config::{Config, ConfigType, NewConfig, NodeConfig};
use crate::model::image::ConfigId;
use crate::model::node::{
    HostCount, Launch, NewNode, NextState, Node, NodeFilter, NodeReport, NodeSearch, NodeSort,
    NodeState, NodeStatus, RegionCount, UpdateNode, UpdateNodeConfig, UpdateNodeState,
};
use crate::model::protocol::ProtocolVersion;
use crate::model::sql::Tag;
use crate::model::{CommandType, Host, Image, Org, Protocol, Region};
use crate::util::{HashVec, NanosUtc};

use super::api::node_service_server::NodeService;
use super::command::node_update;
use super::common::node_launcher;
use super::{api, common, Grpc, Metadata, Status};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Node amount error: {0}
    Amount(#[from] crate::model::sql::amount::Error),
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Auth token parsing failed: {0}
    AuthToken(#[from] crate::auth::token::Error),
    /// Failed to parse block age: {0}
    BlockAge(std::num::TryFromIntError),
    /// Failed to parse block height: {0}
    BlockHeight(std::num::TryFromIntError),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Node command error: {0}
    Command(#[from] crate::model::command::Error),
    /// Node grpc command error: {0}
    CommandGrpc(#[from] crate::grpc::command::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Failed to parse filter limit as i64: {0}
    FilterLimit(std::num::TryFromIntError),
    /// Failed to parse filter offset as i64: {0}
    FilterOffset(std::num::TryFromIntError),
    /// Node host error: {0}
    Host(#[from] crate::model::host::Error),
    /// Node image error: {0}
    Image(#[from] crate::model::image::Error),
    /// Node image config error: {0}
    ImageConfig(#[from] crate::model::image::config::Error),
    /// Node image property error: {0}
    ImageProperty(#[from] crate::model::image::property::Error),
    /// Node ip address error: {0}
    IpAddress(#[from] crate::model::ip_address::Error),
    /// Node launch error: {0}
    Launch(#[from] crate::model::node::launch::Error),
    /// No node ids given.
    MissingIds,
    /// Missing launch type.
    MissingLaunch,
    /// Missing NodeLauncher.
    MissingLauncher,
    /// Node model error: {0}
    Node(#[from] crate::model::node::Error),
    /// Node model status error: {0}
    NodeStatus(#[from] crate::model::node::status::Error),
    /// Node org error: {0}
    Org(#[from] crate::model::org::Error),
    /// Failed to parse ConfigId: {0}
    ParseConfigId(uuid::Error),
    /// Failed to parse HostId: {0}
    ParseHostId(uuid::Error),
    /// Failed to parse NodeId: {0}
    ParseId(uuid::Error),
    /// Failed to parse ImageId: {0}
    ParseImageId(uuid::Error),
    /// Failed to parse ip: {0}
    ParseIp(crate::model::sql::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse ProtocolId: {0}
    ParseProtocolId(uuid::Error),
    /// Failed to parse RegionId: {0}
    ParseRegionId(uuid::Error),
    /// Failed to parse UserId: {0}
    ParseUserId(uuid::Error),
    /// Node protocol error: {0}
    Protocol(#[from] crate::model::protocol::Error),
    /// Node protocol version error: {0}
    ProtocolVersion(#[from] crate::model::protocol::version::Error),
    /// Node region error: {0}
    Region(#[from] crate::model::region::Error),
    /// Node report error: {0}
    Report(#[from] crate::model::node::report::Error),
    /// Report config id `{0}` does not match node config id `{1}`.
    ReportConfigId(ConfigId, ConfigId),
    /// Report status has next_state which is only set by the server.
    ReportNextState,
    /// Node resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// Node firewall rule error: {0}
    Rule(#[from] crate::model::image::rule::Error),
    /// Node search failed: {0}
    SearchOperator(crate::util::search::Error),
    /// Sort order: {0}
    SortOrder(crate::util::search::Error),
    /// Node SQL error: {0}
    Sql(#[from] crate::model::sql::Error),
    /// Node store error: {0}
    Store(#[from] crate::store::Error),
    /// The requested sort field is unknown.
    UnknownSortField,
    /// Node user error: {0}
    User(#[from] crate::model::user::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | Store(_) => Status::internal("Internal error."),
            BlockAge(_) => Status::invalid_argument("block_age"),
            BlockHeight(_) => Status::invalid_argument("block_height"),
            FilterLimit(_) => Status::invalid_argument("limit"),
            FilterOffset(_) => Status::invalid_argument("offset"),
            MissingIds => Status::invalid_argument("ids"),
            MissingLaunch => Status::invalid_argument("launch"),
            MissingLauncher => Status::invalid_argument("launcher"),
            ParseConfigId(_) => Status::invalid_argument("config_id"),
            ParseHostId(_) => Status::invalid_argument("host_id"),
            ParseId(_) => Status::invalid_argument("node_id"),
            ParseImageId(_) => Status::invalid_argument("image_id"),
            ParseIp(_) => Status::invalid_argument("ip_addresses"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseProtocolId(_) => Status::invalid_argument("protocol_id"),
            ParseRegionId(_) => Status::invalid_argument("region_id"),
            ParseUserId(_) => Status::invalid_argument("user_id"),
            ReportConfigId(_, _) => Status::failed_precondition("config_id"),
            ReportNextState => Status::invalid_argument("status.next"),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            Amount(err) => err.into(),
            Auth(err) => err.into(),
            AuthToken(err) => err.into(),
            Claims(err) => err.into(),
            Command(err) => err.into(),
            CommandGrpc(err) => err.into(),
            Host(err) => err.into(),
            Image(err) => err.into(),
            ImageConfig(err) => err.into(),
            ImageProperty(err) => err.into(),
            IpAddress(err) => err.into(),
            Launch(err) => err.into(),
            Node(err) => err.into(),
            NodeStatus(err) => err.into(),
            Org(err) => err.into(),
            Protocol(err) => err.into(),
            ProtocolVersion(err) => err.into(),
            Region(err) => err.into(),
            Report(err) => err.into(),
            Resource(err) => err.into(),
            Rule(err) => err.into(),
            Sql(err) => err.into(),
            User(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl NodeService for Grpc {
    async fn create(
        &self,
        req: Request<api::NodeServiceCreateRequest>,
    ) -> Result<Response<api::NodeServiceCreateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: Request<api::NodeServiceGetRequest>,
    ) -> Result<Response<api::NodeServiceGetResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::NodeServiceListRequest>,
    ) -> Result<Response<api::NodeServiceListResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn report_status(
        &self,
        req: Request<api::NodeServiceReportStatusRequest>,
    ) -> Result<Response<api::NodeServiceReportStatusResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| report_status(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn report_error(
        &self,
        req: Request<api::NodeServiceReportErrorRequest>,
    ) -> Result<Response<api::NodeServiceReportErrorResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| report_error(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn update_config(
        &self,
        req: Request<api::NodeServiceUpdateConfigRequest>,
    ) -> Result<Response<api::NodeServiceUpdateConfigResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_config(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn upgrade_image(
        &self,
        req: Request<api::NodeServiceUpgradeImageRequest>,
    ) -> Result<Response<api::NodeServiceUpgradeImageResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| upgrade_image(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn start(
        &self,
        req: Request<api::NodeServiceStartRequest>,
    ) -> Result<Response<api::NodeServiceStartResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| start(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn stop(
        &self,
        req: Request<api::NodeServiceStopRequest>,
    ) -> Result<Response<api::NodeServiceStopResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| stop(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn restart(
        &self,
        req: Request<api::NodeServiceRestartRequest>,
    ) -> Result<Response<api::NodeServiceRestartResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| restart(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::NodeServiceDeleteRequest>,
    ) -> Result<Response<api::NodeServiceDeleteResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta.into(), write).scope_boxed())
            .await
    }
}

pub async fn create(
    req: api::NodeServiceCreateRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceCreateResponse, Error> {
    let org_id = req.org_id.parse().map_err(Error::ParseOrgId)?;
    let mut perms = vec![Perm::from(NodePerm::Create)];
    let mut resources = vec![Resource::from(org_id)];

    let old_node_id = req
        .old_node_id
        .as_ref()
        .map(|id| id.parse().map_err(Error::ParseId))
        .transpose()?;
    if let Some(old_id) = old_node_id {
        perms.push(Perm::from(CryptPerm::GetSecret));
        resources.push(Resource::from(old_id));
    };

    let launch = req
        .launcher
        .ok_or(Error::MissingLauncher)?
        .launch
        .ok_or(Error::MissingLaunch)?;

    let (launch, authz) = match launch {
        node_launcher::Launch::ByHost(batch) => {
            let host_counts = batch
                .host_counts
                .iter()
                .map(|count| count.try_into().map_err(Into::into))
                .collect::<Result<Vec<HostCount>, Error>>()?;
            for host_count in &host_counts {
                resources.push(Resource::from(host_count.host_id));
            }
            let authz = write
                .auth_or_for(&meta, NodeAdminPerm::Create, perms, &resources)
                .await?;
            (Launch::ByHost(host_counts), authz)
        }

        node_launcher::Launch::ByRegion(batch) => {
            let region_counts = batch
                .region_counts
                .iter()
                .map(|count| count.try_into().map_err(Into::into))
                .collect::<Result<Vec<RegionCount>, Error>>()?;
            let authz = write
                .auth_or_for(&meta, NodeAdminPerm::Create, perms, &resources)
                .await?;
            (Launch::ByRegion(region_counts), authz)
        }
    };

    let image_id = req.image_id.parse().map_err(Error::ParseImageId)?;
    let image = Image::by_id(image_id, Some(org_id), &authz, &mut write).await?;

    let version =
        ProtocolVersion::by_id(image.protocol_version_id, Some(org_id), &authz, &mut write).await?;

    let new_values = req
        .new_values
        .into_iter()
        .map(TryFrom::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let add_rules = req
        .add_rules
        .into_iter()
        .map(TryFrom::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let config = NodeConfig::new(image, Some(org_id), new_values, add_rules, &mut write).await?;

    let new_config = NewConfig {
        image_id,
        archive_id: config.image.archive_id,
        config_type: ConfigType::Node,
        config: config.into(),
    };
    let config = new_config.create(&authz, &mut write).await?;

    let tags = if let Some(ref tags) = req.tags {
        tags.tags
            .iter()
            .map(|tag| Tag::new(tag.name.clone()).map_err(Into::into))
            .collect::<Result<Vec<_>, Error>>()
            .map(Into::into)?
    } else {
        Default::default()
    };

    let new_node = NewNode {
        org_id,
        image_id,
        config_id: config.id,
        old_node_id,
        protocol_id: version.protocol_id,
        protocol_version_id: version.id,
        semantic_version: version.semantic_version,
        auto_upgrade: true,
        tags,
    };

    let created = new_node.create(launch, &authz, &mut write).await?;

    let mut nodes = Vec::with_capacity(created.len());
    for node in created {
        let created_by = common::Resource::from(node.created_by());
        let node_create = NewCommand::node(&node, CommandType::NodeCreate)?
            .create(&mut write)
            .await?;

        let api_cmd = api::Command::from(&node_create, &authz, &mut write).await?;
        let api_node = api::Node::from_model(node, &authz, &mut write).await?;
        let created = api::NodeMessage::created(api_node.clone(), created_by);

        write.mqtt(api_cmd);
        write.mqtt(created);
        nodes.push(api_node);
    }

    Ok(api::NodeServiceCreateResponse { nodes })
}

pub async fn get(
    req: api::NodeServiceGetRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::NodeServiceGetResponse, Error> {
    let node_id = req.node_id.parse().map_err(Error::ParseId)?;
    let authz = read
        .auth_or_for(&meta, NodeAdminPerm::Get, NodePerm::Get, node_id)
        .await?;

    let node = Node::by_id(node_id, &mut read).await?;

    Ok(api::NodeServiceGetResponse {
        node: Some(api::Node::from_model(node, &authz, &mut read).await?),
    })
}

pub async fn list(
    req: api::NodeServiceListRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::NodeServiceListResponse, Error> {
    let filter = req.into_filter()?;
    let authz = if filter.org_ids.is_empty() {
        read.auth(&meta, NodeAdminPerm::List).await?
    } else {
        read.auth_or_for(
            &meta,
            NodeAdminPerm::List,
            NodePerm::List,
            &filter.org_ids[..],
        )
        .await?
    };

    let (nodes, total) = filter.query(&mut read).await?;
    let nodes = api::Node::from_models(nodes, &authz, &mut read).await?;

    Ok(api::NodeServiceListResponse { nodes, total })
}

pub async fn report_status(
    req: api::NodeServiceReportStatusRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceReportStatusResponse, Error> {
    let node_id = req.node_id.parse().map_err(Error::ParseId)?;
    let authz = write
        .auth_or_for(
            &meta,
            NodeAdminPerm::ReportStatus,
            NodePerm::ReportStatus,
            node_id,
        )
        .await?;

    let node = Node::by_id(node_id, &mut write).await?;
    let config_id: ConfigId = req.config_id.parse().map_err(Error::ParseConfigId)?;
    let status: Option<NodeStatus> = req.status.map(TryInto::try_into).transpose()?;

    if node.config_id != config_id {
        return Err(Error::ReportConfigId(config_id, node.config_id));
    } else if status.as_ref().and_then(|status| status.next).is_some() {
        return Err(Error::ReportNextState);
    }

    let update = UpdateNodeState {
        node_state: status.as_ref().map(|status| status.state),
        next_state: None,
        protocol_state: status
            .as_ref()
            .and_then(|status| status.protocol.as_ref())
            .map(|protocol| protocol.state.clone()),
        protocol_health: status
            .as_ref()
            .and_then(|status| status.protocol.as_ref())
            .map(|protocol| protocol.health),
        p2p_address: req.p2p_address.as_deref(),
    };

    let node = update.apply(node_id, &mut write).await?;
    let node = api::Node::from_model(node, &authz, &mut write).await?;

    let updated_by = common::Resource::from(&authz);
    let updated = api::NodeMessage::updated(node, updated_by);

    write.mqtt(updated);

    Ok(api::NodeServiceReportStatusResponse {})
}

pub async fn report_error(
    req: api::NodeServiceReportErrorRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceReportErrorResponse, Error> {
    let node_id: NodeId = req.node_id.parse().map_err(Error::ParseId)?;
    let authz = write
        .auth_or_for(
            &meta,
            NodeAdminPerm::ReportError,
            NodePerm::ReportError,
            node_id,
        )
        .await?;

    let node = Node::by_id(node_id, &mut write).await?;
    let resource = authz.resource();
    let report = node.report(resource, req.message, &mut write).await?;

    Ok(api::NodeServiceReportErrorResponse {
        report_id: report.id.to_string(),
    })
}

pub async fn update_config(
    req: api::NodeServiceUpdateConfigRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceUpdateConfigResponse, Error> {
    let node_id = req.node_id.parse().map_err(Error::ParseId)?;

    let new_org_id = req
        .new_org_id
        .as_deref()
        .map(|id| id.parse().map_err(Error::ParseOrgId))
        .transpose()?;

    let authz = if req.new_org_id.is_some() {
        let perms = [NodeAdminPerm::UpdateConfig, NodeAdminPerm::Transfer];
        write.auth_all(&meta, perms).await?
    } else if req.cost.is_some() {
        let perms = [NodeAdminPerm::UpdateConfig, NodeAdminPerm::ViewCost];
        write.auth(&meta, perms).await?
    } else {
        write
            .auth_or_for(
                &meta,
                NodeAdminPerm::UpdateConfig,
                NodePerm::UpdateConfig,
                node_id,
            )
            .await?
    };

    let node = Node::by_id(node_id, &mut write).await?;
    let update = UpdateNode {
        org_id: new_org_id,
        host_id: None,
        display_name: req.new_display_name.as_deref(),
        auto_upgrade: req.auto_upgrade,
        ip_address: None,
        ip_gateway: None,
        note: req.new_note.as_deref(),
        tags: req
            .update_tags
            .map(|tags| tags.into_update(node.tags))
            .transpose()?
            .flatten(),
        cost: req.cost.map(common::BillingAmount::try_into).transpose()?,
    };
    update.apply(node_id, &authz, &mut write).await?;

    let values_updated = !req.new_values.is_empty();
    if values_updated || req.new_firewall.is_some() {
        let update = UpdateNodeConfig {
            new_values: req
                .new_values
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            new_firewall: req
                .new_firewall
                .clone()
                .map(TryInto::try_into)
                .transpose()?,
        };
        update.apply(node_id, &authz, &mut write).await?;
    }

    let node = Node::by_id(node_id, &mut write).await?;
    let new_values = if values_updated {
        let config = Config::by_id(node.config_id, &mut write).await?;
        config
            .node_config()?
            .image
            .values
            .into_iter()
            .map(Into::into)
            .collect()
    } else {
        vec![]
    };

    let api_update = api::NodeUpdate {
        node_id: node.id.to_string(),
        config_id: node.config_id.to_string(),
        auto_upgrade: req.auto_upgrade,
        new_org_id: new_org_id.map(|id| id.to_string()),
        new_org_name: None,
        new_display_name: req.new_display_name,
        new_note: req.new_note,
        new_values,
        new_firewall: req.new_firewall,
    };
    let node_cmd = NewCommand::node(&node, CommandType::NodeUpdate)?
        .with_protobuf(&api_update)
        .create(&mut write)
        .await?;
    let update_cmd = node_update(&node_cmd, &mut write).await?;
    write.mqtt(update_cmd);

    let api_node = api::Node::from_model(node, &authz, &mut write).await?;
    let updated_by = common::Resource::from(&authz);
    let updated_msg = api::NodeMessage::updated(api_node, updated_by);
    write.mqtt(updated_msg);

    Ok(api::NodeServiceUpdateConfigResponse {})
}

pub async fn upgrade_image(
    req: api::NodeServiceUpgradeImageRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceUpgradeImageResponse, Error> {
    let ids = req
        .node_ids
        .iter()
        .map(|id| id.parse().map_err(Error::ParseId))
        .collect::<Result<HashSet<_>, _>>()?;

    let mut resources: Vec<_> = ids.iter().copied().map(Resource::from).collect();
    if ids.is_empty() {
        return Err(Error::MissingIds);
    }

    let image_id = req.image_id.parse().map_err(Error::ParseImageId)?;
    let org_id = req
        .org_id
        .map(|id| id.parse::<OrgId>().map_err(Error::ParseOrgId))
        .transpose()?;
    if let Some(org_id) = org_id {
        resources.push(org_id.into());
    }

    let authz = write
        .auth_or_for(&meta, NodeAdminPerm::Upgrade, NodePerm::Upgrade, &resources)
        .await?;

    let nodes = Node::by_ids(&ids, &mut write).await?;

    let image = Image::by_id(image_id, org_id, &authz, &mut write).await?;
    let version =
        ProtocolVersion::by_id(image.protocol_version_id, org_id, &authz, &mut write).await?;
    for node in nodes {
        node.notify_upgrade(&image, &version, org_id, &authz, &mut write)
            .await?;
    }

    Ok(api::NodeServiceUpgradeImageResponse {})
}

pub async fn start(
    req: api::NodeServiceStartRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceStartResponse, Error> {
    let node_id: NodeId = req.node_id.parse().map_err(Error::ParseId)?;
    let authz = write
        .auth_or_for(&meta, NodeAdminPerm::Start, NodePerm::Start, node_id)
        .await?;

    let node = Node::by_id(node_id, &mut write).await?;
    let command = NewCommand::node(&node, CommandType::NodeStart)?;
    let command = command.create(&mut write).await?;
    let api_cmd = api::Command::from(&command, &authz, &mut write).await?;
    write.mqtt(api_cmd);

    Ok(api::NodeServiceStartResponse {})
}

pub async fn stop(
    req: api::NodeServiceStopRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceStopResponse, Error> {
    let node_id = req.node_id.parse().map_err(Error::ParseId)?;
    let authz = write
        .auth_or_for(&meta, NodeAdminPerm::Stop, NodePerm::Stop, node_id)
        .await?;

    let node = Node::by_id(node_id, &mut write).await?;
    let command = NewCommand::node(&node, CommandType::NodeStop)?;
    let command = command.create(&mut write).await?;
    let api_cmd = api::Command::from(&command, &authz, &mut write).await?;
    write.mqtt(api_cmd);

    Ok(api::NodeServiceStopResponse {})
}

pub async fn restart(
    req: api::NodeServiceRestartRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceRestartResponse, Error> {
    let node_id = req.node_id.parse().map_err(Error::ParseId)?;
    let authz = write
        .auth_or_for(&meta, NodeAdminPerm::Restart, NodePerm::Restart, node_id)
        .await?;

    let node = Node::by_id(node_id, &mut write).await?;
    let command = NewCommand::node(&node, CommandType::NodeRestart)?;
    let command = command.create(&mut write).await?;
    let api_cmd = api::Command::from(&command, &authz, &mut write).await?;
    write.mqtt(api_cmd);

    Ok(api::NodeServiceRestartResponse {})
}

pub async fn delete(
    req: api::NodeServiceDeleteRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::NodeServiceDeleteResponse, Error> {
    let node_id: NodeId = req.node_id.parse().map_err(Error::ParseId)?;
    let authz = write
        .auth_or_for(&meta, NodeAdminPerm::Delete, NodePerm::Delete, node_id)
        .await?;

    let node = Node::delete(node_id, &mut write).await?;
    let node_cmd = NewCommand::node(&node, CommandType::NodeDelete)?
        .create(&mut write)
        .await?;
    let delete_cmd = api::Command::from(&node_cmd, &authz, &mut write).await?;
    write.mqtt(delete_cmd);

    let deleted_by = common::Resource::from(&authz);
    let deleted = api::NodeMessage::deleted(&node, Some(deleted_by));
    write.mqtt(deleted);

    Ok(api::NodeServiceDeleteResponse {})
}

impl api::Node {
    pub async fn from_model(node: Node, authz: &AuthZ, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let config = Config::by_id(node.config_id, conn).await?;
        let org = Org::by_id(node.org_id, conn).await?;

        let host = Host::by_id(node.host_id, Some(node.org_id), conn).await?;
        let region = Region::by_id(host.region_id, conn).await?;

        let protocol = Protocol::by_id(node.protocol_id, Some(org.id), authz, conn).await?;
        let version =
            ProtocolVersion::by_id(node.protocol_version_id, Some(org.id), authz, conn).await?;
        let reports = NodeReport::by_node(node.id, conn).await?;

        api::Node::new(
            node, &config, &org, &host, &region, &protocol, &version, reports, authz,
        )
    }

    pub async fn from_models(
        nodes: Vec<Node>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let node_ids = nodes.iter().map(|n| n.id).collect();

        let config_ids = nodes.iter().map(|n| n.config_id).collect();
        let configs = Config::by_ids(&config_ids, conn)
            .await?
            .to_map_keep_last(|config| (config.id, config));

        let org_ids = nodes.iter().map(|n| n.org_id).collect();
        let orgs = Org::by_ids(&org_ids, conn)
            .await?
            .to_map_keep_last(|org| (org.id, org));

        let host_ids = nodes.iter().map(|n| n.host_id).collect();
        let hosts = Host::by_ids(&host_ids, &org_ids, conn).await?;
        let region_ids = hosts.iter().map(|h| h.region_id).collect();
        let regions = Region::by_ids(&region_ids, conn)
            .await?
            .to_map_keep_last(|region| (region.id, region));
        let hosts = hosts.to_map_keep_last(|host| (host.id, host));

        let protocol_ids = nodes.iter().map(|n| n.protocol_id).collect();
        let protocol = Protocol::by_ids(&protocol_ids, &org_ids, authz, conn)
            .await?
            .to_map_keep_last(|chain| (chain.id, chain));

        let version_ids = nodes.iter().map(|n| n.protocol_version_id).collect();
        let versions = ProtocolVersion::by_ids(&version_ids, &org_ids, authz, conn)
            .await?
            .to_map_keep_last(|version| (version.id, version));

        let mut reports = NodeReport::by_node_ids(&node_ids, conn)
            .await?
            .to_map_keep_all(|report| (report.node_id, report));

        nodes
            .into_iter()
            .filter_map(|node| {
                let config = configs.get(&node.config_id)?;
                let org = orgs.get(&node.org_id)?;
                let host = hosts.get(&node.host_id)?;
                let region = regions.get(&host.region_id)?;
                let protocol = protocol.get(&node.protocol_id)?;
                let version = versions.get(&node.protocol_version_id)?;
                let reports = reports.remove(&node.id).unwrap_or_default();

                Some(api::Node::new(
                    node, config, org, host, region, protocol, version, reports, authz,
                ))
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        node: Node,
        config: &Config,
        org: &Org,
        host: &Host,
        region: &Region,
        protocol: &Protocol,
        version: &ProtocolVersion,
        reports: Vec<NodeReport>,
        authz: &AuthZ,
    ) -> Result<Self, Error> {
        let config = config.node_config()?;
        let status = node.status();
        let created_by = node.created_by();
        let cost = common::BillingAmount::from_node(&node, authz);

        let block_height = node
            .block_height
            .map(|height| u64::try_from(height).map_err(Error::BlockHeight))
            .transpose()?;
        let block_age = node
            .block_height
            .map(|age| u64::try_from(age).map_err(Error::BlockAge))
            .transpose()?;

        let jobs = node
            .jobs
            .map(|jobs| jobs.into_iter().map(Into::into).collect())
            .unwrap_or_default();
        let reports = reports
            .into_iter()
            .map(|report| {
                let created_by = report.created_by();
                common::NodeReport {
                    report_id: report.id.to_string(),
                    message: report.message,
                    created_by: Some(common::Resource::from(created_by)),
                    created_at: Some(NanosUtc::from(report.created_at).into()),
                }
            })
            .collect();

        Ok(api::Node {
            node_id: node.id.to_string(),
            org_id: node.org_id.to_string(),
            org_name: org.name.clone(),
            node_name: node.node_name,
            display_name: node.display_name,
            old_node_id: node.old_node_id.map(|id| id.to_string()),
            image_id: node.image_id.to_string(),
            config_id: node.config_id.to_string(),
            config: Some(config.into()),
            host_id: node.host_id.to_string(),
            host_org_id: host.org_id.map(|id| id.to_string()),
            host_network_name: host.network_name.clone(),
            host_display_name: host.display_name.clone(),
            region_id: region.id.to_string(),
            region_key: region.key.clone().into(),
            region_name: region.display_name.clone(),
            protocol_id: node.protocol_id.to_string(),
            protocol_name: protocol.name.clone(),
            protocol_version_id: node.protocol_version_id.to_string(),
            version_key: Some(common::ProtocolVersionKey {
                protocol_key: version.protocol_key.to_string(),
                variant_key: version.variant_key.to_string(),
            }),
            semantic_version: node.semantic_version.to_string(),
            auto_upgrade: node.auto_upgrade,
            ip_address: node.ip_address.to_string(),
            ip_gateway: node.ip_gateway.to_string(),
            dns_name: node.dns_name,
            p2p_address: node.p2p_address,
            dns_url: node.dns_url,
            block_height,
            block_age,
            note: node.note,
            node_status: Some(status.into()),
            jobs,
            reports,
            tags: Some(node.tags.into()),
            created_by: Some(common::Resource::from(created_by)),
            created_at: Some(NanosUtc::from(node.created_at).into()),
            updated_at: node.updated_at.map(NanosUtc::from).map(Into::into),
            cost,
        })
    }
}

impl api::NodeServiceListRequest {
    fn into_filter(self) -> Result<NodeFilter, Error> {
        let node_states = self
            .node_states()
            .map(NodeState::try_from)
            .collect::<Result<_, _>>()?;
        let next_states = self
            .next_states()
            .map(NextState::try_from)
            .collect::<Result<_, _>>()?;

        let protocol_ids = self
            .protocol_ids
            .iter()
            .map(|id| id.parse().map_err(Error::ParseProtocolId))
            .collect::<Result<_, _>>()?;
        let version_keys = self
            .version_keys
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;
        let org_ids = self
            .org_ids
            .iter()
            .map(|id| id.parse().map_err(Error::ParseOrgId))
            .collect::<Result<_, _>>()?;
        let host_ids = self
            .host_ids
            .iter()
            .map(|id| id.parse().map_err(Error::ParseHostId))
            .collect::<Result<_, _>>()?;
        let user_ids = self
            .user_ids
            .iter()
            .map(|id| id.parse().map_err(Error::ParseUserId))
            .collect::<Result<_, _>>()?;

        let search = self
            .search
            .map(|search| {
                Ok::<_, Error>(NodeSearch {
                    operator: search
                        .operator()
                        .try_into()
                        .map_err(Error::SearchOperator)?,
                    id: search.node_id.map(|id| id.trim().to_lowercase()),
                    node_name: search.node_name.map(|name| name.trim().to_lowercase()),
                    display_name: search.display_name.map(|name| name.trim().to_lowercase()),
                    dns_name: search.dns_name.map(|name| name.trim().to_lowercase()),
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
                    api::NodeSortField::NodeName => Ok(NodeSort::NodeName(order)),
                    api::NodeSortField::DnsName => Ok(NodeSort::DnsName(order)),
                    api::NodeSortField::DisplayName => Ok(NodeSort::DisplayName(order)),
                    api::NodeSortField::NodeState => Ok(NodeSort::NodeState(order)),
                    api::NodeSortField::NextState => Ok(NodeSort::NextState(order)),
                    api::NodeSortField::ProtocolState => Ok(NodeSort::ProtocolState(order)),
                    api::NodeSortField::ProtocolHealth => Ok(NodeSort::ProtocolHealth(order)),
                    api::NodeSortField::BlockHeight => Ok(NodeSort::BlockHeight(order)),
                    api::NodeSortField::CreatedAt => Ok(NodeSort::CreatedAt(order)),
                    api::NodeSortField::UpdatedAt => Ok(NodeSort::UpdatedAt(order)),
                }
            })
            .collect::<Result<_, _>>()?;

        let ip_addresses = self
            .ip_addresses
            .iter()
            .map(|ip| ip.parse().map_err(Error::ParseIp))
            .collect::<Result<_, _>>()?;

        Ok(NodeFilter {
            protocol_ids,
            version_keys,
            semantic_versions: self.semantic_versions,
            org_ids,
            host_ids,
            user_ids,
            ip_addresses,
            node_states,
            next_states,
            search,
            sort,
            limit: i64::try_from(self.limit).map_err(Error::FilterLimit)?,
            offset: i64::try_from(self.offset).map_err(Error::FilterOffset)?,
        })
    }
}
