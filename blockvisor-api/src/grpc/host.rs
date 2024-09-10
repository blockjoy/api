use std::cmp::max;
use std::collections::{HashMap, HashSet};

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::claims::Claims;
use crate::auth::rbac::{GrpcRole, HostAdminPerm, HostPerm};
use crate::auth::resource::{HostId, OrgId, Resource};
use crate::auth::token::refresh::Refresh;
use crate::auth::Authorize;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::model::command::NewCommand;
use crate::model::host::{
    Host, HostFilter, HostRequirements, HostSearch, HostSort, NewHost, UpdateHost,
};
use crate::model::node::NodeScheduler;
use crate::model::protocol::ProtocolVersion;
use crate::model::{CommandType, Image, IpAddress, Node, Org, Protocol, Region, RegionId, Token};
use crate::util::sql::{Tag, Tags, Version};
use crate::util::{HashVec, NanosUtc};

use super::api::host_service_server::HostService;
use super::{api, common, Grpc, Metadata, Status};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Host command error: {0}
    Command(#[from] crate::model::command::Error),
    /// Host command API error: {0}
    CommandApi(#[from] crate::grpc::command::Error),
    /// Failed to parse cpu cores: {0}
    CpuCores(std::num::TryFromIntError),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Failed to parse disk bytes: {0}
    DiskBytes(std::num::TryFromIntError),
    /// This host cannot be deleted because it still has nodes.
    HasNodes,
    /// Host model error: {0}
    Host(#[from] crate::model::host::Error),
    /// Host token error: {0}
    HostProvisionByToken(crate::model::token::Error),
    /// Host image error: {0}
    Image(#[from] crate::model::image::Error),
    /// Host ip address error: {0}
    IpAddress(#[from] crate::model::ip_address::Error),
    /// Host JWT failure: {0}
    Jwt(#[from] crate::auth::token::jwt::Error),
    /// Failed to parse memory bytes: {0}
    MemoryBytes(std::num::TryFromIntError),
    /// Node model error: {0}
    Node(#[from] crate::model::node::Error),
    /// Host org error: {0}
    Org(#[from] crate::model::org::Error),
    /// Failed to parse bv_version: {0}
    ParseBvVersion(crate::util::sql::Error),
    /// Failed to parse HostId: {0}
    ParseId(uuid::Error),
    /// Failed to parse ImageId: {0}
    ParseImageId(uuid::Error),
    /// Failed to parse ip: {0}
    ParseIps(crate::util::sql::Error),
    /// Failed to parse IP address: {0}
    ParseIpAddress(crate::util::sql::Error),
    /// Failed to parse IP gateway: {0}
    ParseIpGateway(crate::util::sql::Error),
    /// Failed to parse non-zero host node_count as u64: {0}
    ParseNodeCount(std::num::TryFromIntError),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Host protocol error: {0}
    Protocol(#[from] crate::model::protocol::Error),
    /// Host protocol version error: {0}
    ProtocolVersion(#[from] crate::model::protocol::version::Error),
    /// Host Refresh token failure: {0}
    Refresh(#[from] crate::auth::token::refresh::Error),
    /// Host region error: {0}
    Region(#[from] crate::model::region::Error),
    /// Host search failed: {0}
    SearchOperator(crate::util::search::Error),
    /// Sort order: {0}
    SortOrder(crate::util::search::Error),
    /// Host SQL error: {0}
    Sql(#[from] crate::util::sql::Error),
    /// Host store error: {0}
    Store(#[from] crate::store::Error),
    /// The requested sort field is unknown.
    UnknownSortField,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | Jwt(_) | ParseNodeCount(_) | Refresh(_) => {
                Status::internal("Internal error.")
            }
            CpuCores(_) => Status::out_of_range("cpu_cores"),
            DiskBytes(_) => Status::out_of_range("disk_bytes"),
            HasNodes => Status::failed_precondition("This host still has nodes."),
            HostProvisionByToken(_) => Status::forbidden("Invalid token."),
            MemoryBytes(_) => Status::out_of_range("memory_bytes"),
            ParseBvVersion(_) => Status::invalid_argument("bv_version"),
            ParseId(_) => Status::invalid_argument("id"),
            ParseImageId(_) => Status::invalid_argument("image_id"),
            ParseIps(_) => Status::invalid_argument("ips"),
            ParseIpAddress(_) => Status::invalid_argument("ip_address"),
            ParseIpGateway(_) => Status::invalid_argument("ip_gateway"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Command(err) => err.into(),
            CommandApi(err) => err.into(),
            Host(err) => err.into(),
            Image(err) => err.into(),
            IpAddress(err) => err.into(),
            Node(err) => err.into(),
            Org(err) => err.into(),
            Protocol(err) => err.into(),
            ProtocolVersion(err) => err.into(),
            Region(err) => err.into(),
            Sql(err) => err.into(),
            Store(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl HostService for Grpc {
    async fn create(
        &self,
        req: Request<api::HostServiceCreateRequest>,
    ) -> Result<Response<api::HostServiceCreateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: Request<api::HostServiceGetRequest>,
    ) -> Result<Response<api::HostServiceGetResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::HostServiceListRequest>,
    ) -> Result<Response<api::HostServiceListResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn update(
        &self,
        req: Request<api::HostServiceUpdateRequest>,
    ) -> Result<Response<api::HostServiceUpdateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::HostServiceDeleteRequest>,
    ) -> Result<Response<api::HostServiceDeleteResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn start(
        &self,
        req: Request<api::HostServiceStartRequest>,
    ) -> Result<Response<api::HostServiceStartResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| start(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn stop(
        &self,
        req: Request<api::HostServiceStopRequest>,
    ) -> Result<Response<api::HostServiceStopResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| stop(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn restart(
        &self,
        req: Request<api::HostServiceRestartRequest>,
    ) -> Result<Response<api::HostServiceRestartResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| restart(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn regions(
        &self,
        req: Request<api::HostServiceRegionsRequest>,
    ) -> Result<Response<api::HostServiceRegionsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| regions(req, meta.into(), read).scope_boxed())
            .await
    }
}

pub async fn create(
    req: api::HostServiceCreateRequest,
    _meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceCreateResponse, Error> {
    let token = Token::host_provision_by_token(&req.provision_token, &mut write)
        .await
        .map_err(Error::HostProvisionByToken)?;
    let org_id = req.is_private.then_some(token.org_id);

    let host_ips: Vec<_> = req
        .ips
        .iter()
        .map(|ip| ip.parse().map_err(Error::ParseIps))
        .collect::<Result<_, _>>()?;

    let region = if let Some(ref region) = req.region {
        Region::get_or_create(region, None, &mut write)
            .await
            .map(Some)?
    } else {
        None
    };

    let tags = if let Some(ref tags) = req.tags {
        tags.tags
            .iter()
            .map(|tag| Tag::new(tag.name.clone()).map_err(Into::into))
            .collect::<Result<Vec<_>, Error>>()
            .map(Tags)?
    } else {
        Default::default()
    };

    let new_host = NewHost {
        org_id,
        network_name: &req.network_name,
        display_name: req.display_name.as_deref(),
        region_id: region.map(|region| region.id),
        schedule_type: req.schedule_type().try_into()?,
        os: &req.os,
        os_version: &req.os_version,
        bv_version: &req.bv_version.parse().map_err(Error::ParseBvVersion)?,
        ip_address: req.ip_address.parse().map_err(Error::ParseIpAddress)?,
        ip_gateway: req.ip_gateway.parse().map_err(Error::ParseIpGateway)?,
        cpu_cores: req.cpu_cores.try_into().map_err(Error::CpuCores)?,
        memory_bytes: req.memory_bytes.try_into().map_err(Error::MemoryBytes)?,
        disk_bytes: req.disk_bytes.try_into().map_err(Error::DiskBytes)?,
        tags,
        created_by_type: token.created_by_type,
        created_by_id: token.created_by_id,
    };
    let host = new_host.create(&host_ips, &mut write).await?;

    let expire_token = write.ctx.config.token.expire.token;
    let expire_refresh = write.ctx.config.token.expire.refresh_host;

    let claims = Claims::from_now(expire_token, host.id, GrpcRole::NewHost);
    let jwt = write.ctx.auth.cipher.jwt.encode(&claims)?;

    let refresh = Refresh::from_now(expire_refresh, host.id);
    let encoded = write.ctx.auth.cipher.refresh.encode(&refresh)?;

    let host = api::Host::from_host(host, &mut write).await?;

    Ok(api::HostServiceCreateResponse {
        host: Some(host),
        token: jwt.into(),
        refresh: encoded.into(),
        provision_org_id: token.org_id.to_string(),
    })
}

pub async fn get(
    req: api::HostServiceGetRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceGetResponse, Error> {
    let id: HostId = req.host_id.parse().map_err(Error::ParseId)?;
    let mut resources = vec![Resource::from(id)];

    let org_id = Host::org_id(id, &mut read).await?;
    let _authz = if let Some(org_id) = org_id {
        resources.push(Resource::from(org_id));
        read.auth_or_for(&meta, HostAdminPerm::Get, HostPerm::Get, &resources)
            .await?
    } else {
        read.auth(&meta, HostAdminPerm::Get).await?
    };

    let host = Host::by_id(id, org_id, &mut read).await?;
    let host = api::Host::from_host(host, &mut read).await?;

    Ok(api::HostServiceGetResponse { host: Some(host) })
}

pub async fn list(
    req: api::HostServiceListRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceListResponse, Error> {
    let filter = req.into_filter()?;
    let _authz = if filter.org_ids.is_empty() {
        read.auth(&meta, HostAdminPerm::List).await?
    } else {
        read.auth_or_for(&meta, HostAdminPerm::List, HostPerm::List, &filter.org_ids)
            .await?
    };

    let (hosts, _) = filter.query(&mut read).await?;
    let hosts = api::Host::from_hosts(hosts, &mut read).await?;

    Ok(api::HostServiceListResponse { hosts })
}

pub async fn update(
    req: api::HostServiceUpdateRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceUpdateResponse, Error> {
    let id: HostId = req.host_id.parse().map_err(Error::ParseId)?;
    let mut resources = vec![Resource::from(id)];

    let org_id = Host::org_id(id, &mut write).await?;
    if let Some(org_id) = org_id {
        resources.push(Resource::from(org_id));
    };

    let _authz = write
        .auth_or_for(&meta, HostAdminPerm::Update, HostPerm::Update, &resources)
        .await?;
    let host = Host::by_id(id, org_id, &mut write).await?;

    let bv_version = req
        .bv_version
        .as_ref()
        .map(|bv| bv.parse::<Version>().map_err(Error::ParseBvVersion))
        .transpose()?;
    let disk_bytes = req
        .disk_bytes
        .map(|space| space.try_into().map_err(Error::DiskBytes))
        .transpose()?;
    let region = if let Some(ref region) = req.region {
        Region::get_or_create(region, None, &mut write)
            .await
            .map(Some)?
    } else {
        None
    };

    let update_host = UpdateHost {
        id: req.host_id.parse().map_err(Error::ParseId)?,
        network_name: req.network_name.as_deref(),
        display_name: req.display_name.as_deref(),
        region_id: region.map(|r| r.id),
        schedule_type: req
            .schedule_type
            .map(|_| req.schedule_type().try_into())
            .transpose()?,
        connection_status: None,
        os: req.os.as_deref(),
        os_version: req.os_version.as_deref(),
        bv_version: bv_version.as_ref(),
        ip_address: None,
        ip_gateway: None,
        cpu_cores: None,
        memory_bytes: None,
        disk_bytes,
        tags: req
            .update_tags
            .map(|tags| tags.into_update(host.tags))
            .transpose()?
            .flatten(),
    };
    let updated = update_host.update(&mut write).await?;
    let host = api::Host::from_host(updated, &mut write).await?;

    Ok(api::HostServiceUpdateResponse { host: Some(host) })
}

pub async fn delete(
    req: api::HostServiceDeleteRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceDeleteResponse, Error> {
    let id: HostId = req.host_id.parse().map_err(Error::ParseId)?;
    let mut resources = vec![Resource::from(id)];

    let org_id = Host::org_id(id, &mut write).await?;
    let _authz = if let Some(org_id) = org_id {
        resources.push(Resource::from(org_id));
        write
            .auth_or_for(&meta, HostAdminPerm::Delete, HostPerm::Delete, &resources)
            .await?
    } else {
        write.auth(&meta, HostAdminPerm::Delete).await?
    };

    if Node::host_has_nodes(id, &mut write).await? {
        return Err(Error::HasNodes);
    }

    Host::delete(id, org_id, &mut write).await?;
    IpAddress::delete_by_host_id(id, &mut write).await?;

    Ok(api::HostServiceDeleteResponse {})
}

pub async fn start(
    req: api::HostServiceStartRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceStartResponse, Error> {
    let id: HostId = req.host_id.parse().map_err(Error::ParseId)?;
    let mut resources = vec![Resource::from(id)];

    let org_id = Host::org_id(id, &mut write).await?;
    let _authz = if let Some(org_id) = org_id {
        resources.push(Resource::from(org_id));
        write
            .auth_or_for(&meta, HostAdminPerm::Start, HostPerm::Start, &resources)
            .await?
    } else {
        write.auth(&meta, HostAdminPerm::Start).await?
    };

    let command = NewCommand::host(id, CommandType::HostStart)?;
    let command = command.create(&mut write).await?;
    let message = api::Command::from_host(&command)?;
    write.mqtt(message);

    Ok(api::HostServiceStartResponse {})
}

pub async fn stop(
    req: api::HostServiceStopRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceStopResponse, Error> {
    let id: HostId = req.host_id.parse().map_err(Error::ParseId)?;
    let mut resources = vec![Resource::from(id)];

    let org_id = Host::org_id(id, &mut write).await?;
    let _authz = if let Some(org_id) = org_id {
        resources.push(Resource::from(org_id));
        write
            .auth_or_for(&meta, HostAdminPerm::Stop, HostPerm::Stop, &resources)
            .await?
    } else {
        write.auth(&meta, HostAdminPerm::Stop).await?
    };

    let command = NewCommand::host(id, CommandType::HostStop)?;
    let command = command.create(&mut write).await?;
    let message = api::Command::from_host(&command)?;
    write.mqtt(message);

    Ok(api::HostServiceStopResponse {})
}

pub async fn restart(
    req: api::HostServiceRestartRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceRestartResponse, Error> {
    let id: HostId = req.host_id.parse().map_err(Error::ParseId)?;
    let mut resources = vec![Resource::from(id)];

    let org_id = Host::org_id(id, &mut write).await?;
    let _authz = if let Some(org_id) = org_id {
        resources.push(Resource::from(org_id));
        write
            .auth_or_for(&meta, HostAdminPerm::Restart, HostPerm::Restart, &resources)
            .await?
    } else {
        write.auth(&meta, HostAdminPerm::Restart).await?
    };

    let command = NewCommand::host(id, CommandType::HostRestart)?;
    let command = command.create(&mut write).await?;
    let message = api::Command::from_host(&command)?;
    write.mqtt(message);

    Ok(api::HostServiceRestartResponse {})
}

pub async fn regions(
    req: api::HostServiceRegionsRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceRegionsResponse, Error> {
    let org_id = req
        .org_id
        .as_ref()
        .map(|id| id.parse().map_err(Error::ParseOrgId))
        .transpose()?;

    let authz = if let Some(org_id) = org_id {
        read.auth_or_for(&meta, HostAdminPerm::Regions, HostPerm::Regions, org_id)
            .await?
    } else {
        read.auth(&meta, HostAdminPerm::Regions).await?
    };

    let image_id = req.image_id.parse().map_err(Error::ParseImageId)?;
    let image = Image::by_id(image_id, org_id, &authz, &mut read).await?;
    let version =
        ProtocolVersion::by_id(image.protocol_version_id, org_id, &authz, &mut read).await?;
    let protocol = Protocol::by_id(version.protocol_id, org_id, &authz, &mut read).await?;

    let requirements = HostRequirements {
        scheduler: NodeScheduler::least_resources(),
        protocol: &protocol,
        org_id,
        cpu_cores: image.min_cpu_cores,
        memory_bytes: image.min_memory_bytes,
        disk_bytes: image.min_disk_bytes,
    };

    let candidates = Host::candidates(requirements, None, &mut read).await?;
    let region_ids = candidates
        .into_iter()
        .filter_map(|host| host.region_id)
        .collect();

    let mut regions = Region::by_ids(&region_ids, &mut read).await?;
    regions.sort_by(|r1, r2| r1.name.cmp(&r2.name));

    let regions = regions
        .into_iter()
        .map(|region| api::Region {
            name: Some(region.name),
            pricing_tier: region.pricing_tier,
        })
        .collect();

    Ok(api::HostServiceRegionsResponse { regions })
}

impl api::Host {
    pub async fn from_host(host: Host, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let lookup = Lookup::from_host(&host, conn).await?;
        Self::from_model(host, &lookup)
    }

    pub async fn from_hosts(hosts: Vec<Host>, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        let lookup = Lookup::from_hosts(&hosts, conn).await?;

        let mut out = Vec::new();
        for host in hosts {
            out.push(Self::from_model(host, &lookup)?);
        }

        Ok(out)
    }

    fn from_model(host: Host, lookup: &Lookup) -> Result<Self, Error> {
        let created_by = host.created_by();
        let org = host.org_id.and_then(|id| lookup.orgs.get(&id));
        let org_name = org.map(|org| org.name.clone());
        let region = host.region_id.and_then(|id| lookup.regions.get(&id));
        let region = region.map(|region| region.name.clone());

        let no_ips = vec![];
        let no_nodes = vec![];
        let ips = lookup.ip_addresses.get(&host.id).unwrap_or(&no_ips);
        let nodes = lookup.nodes.get(&host.id).unwrap_or(&no_nodes);
        let ip_addresses = ips
            .iter()
            .map(|ip_address| api::HostIpAddress {
                ip: ip_address.ip.to_string(),
                assigned: nodes.iter().any(|node| node.ip_address == ip_address.ip),
            })
            .collect();

        Ok(api::Host {
            host_id: host.id.to_string(),
            org_id: host.org_id.map(|id| id.to_string()),
            org_name,
            region,
            network_name: host.network_name,
            display_name: host.display_name,
            schedule_type: api::ScheduleType::from(host.schedule_type).into(),
            os: host.os,
            os_version: host.os_version,
            bv_version: host.bv_version.to_string(),
            ip_address: host.ip_address.to_string(),
            ip_gateway: host.ip_gateway.to_string(),
            ip_addresses,
            cpu_cores: host.cpu_cores.try_into().map_err(Error::CpuCores)?,
            memory_bytes: host.memory_bytes.try_into().map_err(Error::MemoryBytes)?,
            disk_bytes: host.disk_bytes.try_into().map_err(Error::DiskBytes)?,
            node_count: u64::try_from(max(0, host.node_count)).map_err(Error::ParseNodeCount)?,
            tags: Some(host.tags.into()),
            created_by: Some(common::Resource::from(created_by)),
            created_at: Some(NanosUtc::from(host.created_at).into()),
            updated_at: host.updated_at.map(|at| NanosUtc::from(at).into()),
        })
    }
}

struct Lookup {
    orgs: HashMap<OrgId, Org>,
    nodes: HashMap<HostId, Vec<Node>>,
    regions: HashMap<RegionId, Region>,
    ip_addresses: HashMap<HostId, Vec<IpAddress>>,
}

impl Lookup {
    async fn from_host(host: &Host, conn: &mut Conn<'_>) -> Result<Lookup, Error> {
        Self::from_hosts(&[host], conn).await
    }

    async fn from_hosts<H>(hosts: &[H], conn: &mut Conn<'_>) -> Result<Lookup, Error>
    where
        H: AsRef<Host> + Send + Sync,
    {
        let host_ids: HashSet<HostId> = hosts.iter().map(|h| h.as_ref().id).collect();

        let org_ids = hosts.iter().filter_map(|h| h.as_ref().org_id).collect();
        let orgs = Org::by_ids(&org_ids, conn)
            .await?
            .to_map_keep_last(|org| (org.id, org));

        let region_ids = hosts.iter().filter_map(|h| h.as_ref().region_id).collect();
        let regions = Region::by_ids(&region_ids, conn)
            .await?
            .to_map_keep_last(|region| (region.id, region));

        let ip_addresses = IpAddress::by_host_ids(&host_ids, conn)
            .await?
            .into_iter()
            .map(|ip| (ip.host_id, ip))
            .to_map_keep_all(|(host_id, ip)| (host_id, ip));

        let nodes = Node::by_host_ids(&host_ids, &org_ids, conn)
            .await?
            .to_map_keep_all(|node| (node.host_id, node));

        Ok(Lookup {
            orgs,
            nodes,
            regions,
            ip_addresses,
        })
    }
}

impl api::HostServiceListRequest {
    fn into_filter(self) -> Result<HostFilter, Error> {
        let org_ids = self
            .org_ids
            .into_iter()
            .map(|id| id.parse().map_err(Error::ParseOrgId))
            .collect::<Result<_, _>>()?;
        let versions = self
            .bv_versions
            .into_iter()
            .map(|v| v.trim().to_lowercase())
            .collect();

        let search = self
            .search
            .map(|search| {
                Ok::<_, Error>(HostSearch {
                    operator: search
                        .operator()
                        .try_into()
                        .map_err(Error::SearchOperator)?,
                    id: search.host_id.map(|id| id.trim().to_lowercase()),
                    network_name: search.network_name.map(|name| name.trim().to_lowercase()),
                    display_name: search.display_name.map(|name| name.trim().to_lowercase()),
                    bv_version: search
                        .bv_version
                        .map(|version| version.trim().to_lowercase()),
                    os: search.os.map(|os| os.trim().to_lowercase()),
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
                    api::HostSortField::Unspecified => Err(Error::UnknownSortField),
                    api::HostSortField::NetworkName => Ok(HostSort::NetworkName(order)),
                    api::HostSortField::DisplayName => Ok(HostSort::DisplayName(order)),
                    api::HostSortField::Os => Ok(HostSort::Os(order)),
                    api::HostSortField::OsVersion => Ok(HostSort::OsVersion(order)),
                    api::HostSortField::BvVersion => Ok(HostSort::BvVersion(order)),
                    api::HostSortField::CpuCores => Ok(HostSort::CpuCores(order)),
                    api::HostSortField::MemoryBytes => Ok(HostSort::MemoryBytes(order)),
                    api::HostSortField::DiskBytes => Ok(HostSort::DiskBytes(order)),
                    api::HostSortField::NodeCount => Ok(HostSort::NodeCount(order)),
                    api::HostSortField::CreatedAt => Ok(HostSort::CreatedAt(order)),
                    api::HostSortField::UpdatedAt => Ok(HostSort::UpdatedAt(order)),
                }
            })
            .collect::<Result<_, _>>()?;

        Ok(HostFilter {
            org_ids,
            versions,
            offset: self.offset,
            limit: self.limit,
            search,
            sort,
        })
    }
}
