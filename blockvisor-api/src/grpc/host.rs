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
use crate::auth::{AuthZ, Authorize};
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::model::command::NewCommand;
use crate::model::host::{
    Host, HostFilter, HostRequirements, HostSearch, HostSort, NewHost, UpdateHost,
};
use crate::model::node::NodeScheduler;
use crate::model::region::{NewRegion, RegionKey, UpdateRegion};
use crate::model::sql::{IpNetwork, Tag, Version};
use crate::model::{
    CommandType, Image, IpAddress, Node, Org, Protocol, ProtocolVersion, Region, RegionId, Token,
};
use crate::util::{HashVec, NanosUtc};

use super::api::host_service_server::HostService;
use super::{Grpc, Metadata, Status, api, common};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Host amount error: {0}
    Amount(#[from] crate::model::sql::amount::Error),
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
    /// Failed to parse filter limit as i64: {0}
    FilterLimit(std::num::TryFromIntError),
    /// Failed to parse filter offset as i64: {0}
    FilterOffset(std::num::TryFromIntError),
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
    /// Lookup missing Region. This should not happen.
    LookupMissingRegion,
    /// Failed to parse memory bytes: {0}
    MemoryBytes(std::num::TryFromIntError),
    /// Missing the region to get info for.
    MissingRegion,
    /// Node model error: {0}
    Node(#[from] crate::model::node::Error),
    /// No visibility of HostRestart command.
    NoHostRestart,
    /// No visibility of HostStart command.
    NoHostStart,
    /// No visibility of HostStop command.
    NoHostStop,
    /// Host org error: {0}
    Org(#[from] crate::model::org::Error),
    /// Failed to parse bv_version: {0}
    ParseBvVersion(crate::model::sql::Error),
    /// Failed to parse HostId: {0}
    ParseId(uuid::Error),
    /// Failed to parse ImageId: {0}
    ParseImageId(uuid::Error),
    /// Failed to parse ip: {0}
    ParseIps(crate::model::sql::Error),
    /// Failed to parse IP address: {0}
    ParseIpAddress(crate::model::sql::Error),
    /// Failed to parse IP gateway: {0}
    ParseIpGateway(crate::model::sql::Error),
    /// Failed to parse non-zero host node_count as u64: {0}
    ParseNodeCount(std::num::TryFromIntError),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse RegionId: {0}
    ParseRegionId(uuid::Error),
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
    Sql(#[from] crate::model::sql::Error),
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
            Diesel(_) | Jwt(_) | LookupMissingRegion | ParseNodeCount(_) | Refresh(_) => {
                Status::internal("Internal error.")
            }
            CpuCores(_) => Status::out_of_range("cpu_cores"),
            DiskBytes(_) => Status::out_of_range("disk_bytes"),
            FilterLimit(_) => Status::invalid_argument("limit"),
            FilterOffset(_) => Status::invalid_argument("offset"),
            HasNodes => Status::failed_precondition("This host still has nodes."),
            HostProvisionByToken(_) => Status::forbidden("Invalid token."),
            MemoryBytes(_) => Status::out_of_range("memory_bytes"),
            MissingRegion => Status::out_of_range("region"),
            NoHostRestart | NoHostStart | NoHostStop => Status::forbidden("Access denied."),
            ParseBvVersion(_) => Status::invalid_argument("bv_version"),
            ParseId(_) => Status::invalid_argument("host_id"),
            ParseImageId(_) => Status::invalid_argument("image_id"),
            ParseIps(_) => Status::invalid_argument("ips"),
            ParseIpAddress(_) => Status::invalid_argument("ip_address"),
            ParseIpGateway(_) => Status::invalid_argument("ip_gateway"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseRegionId(_) => Status::invalid_argument("region_id"),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            Amount(err) => err.into(),
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
    async fn create_host(
        &self,
        req: Request<api::HostServiceCreateHostRequest>,
    ) -> Result<Response<api::HostServiceCreateHostResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create_host(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn create_region(
        &self,
        req: Request<api::HostServiceCreateRegionRequest>,
    ) -> Result<Response<api::HostServiceCreateRegionResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create_region(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn get_host(
        &self,
        req: Request<api::HostServiceGetHostRequest>,
    ) -> Result<Response<api::HostServiceGetHostResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_host(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn get_region(
        &self,
        req: Request<api::HostServiceGetRegionRequest>,
    ) -> Result<Response<api::HostServiceGetRegionResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_region(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn list_hosts(
        &self,
        req: Request<api::HostServiceListHostsRequest>,
    ) -> Result<Response<api::HostServiceListHostsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_hosts(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn list_regions(
        &self,
        req: Request<api::HostServiceListRegionsRequest>,
    ) -> Result<Response<api::HostServiceListRegionsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_regions(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn update_host(
        &self,
        req: Request<api::HostServiceUpdateHostRequest>,
    ) -> Result<Response<api::HostServiceUpdateHostResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_host(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn update_region(
        &self,
        req: Request<api::HostServiceUpdateRegionRequest>,
    ) -> Result<Response<api::HostServiceUpdateRegionResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_region(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn delete_host(
        &self,
        req: Request<api::HostServiceDeleteHostRequest>,
    ) -> Result<Response<api::HostServiceDeleteHostResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete_host(req, meta.into(), write).scope_boxed())
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
}

pub async fn create_host(
    req: api::HostServiceCreateHostRequest,
    _meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceCreateHostResponse, Error> {
    let token = Token::host_provision_by_token(&req.provision_token, &mut write)
        .await
        .map_err(Error::HostProvisionByToken)?;
    let org_id = req.is_private.then_some(token.org_id);
    let region_id = req.region_id.parse().map_err(Error::ParseRegionId)?;

    let host_ips: Vec<_> = req
        .ips
        .iter()
        .map(|ip| ip.parse().map_err(Error::ParseIps))
        .collect::<Result<_, _>>()?;

    let tags = if let Some(ref tags) = req.tags {
        tags.tags
            .iter()
            .map(|tag| Tag::new(tag.name.clone()).map_err(Into::into))
            .collect::<Result<Vec<_>, Error>>()
            .map(Into::into)?
    } else {
        Default::default()
    };

    let new_host = NewHost {
        org_id,
        region_id,
        network_name: &req.network_name,
        display_name: req.display_name.as_deref(),
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

    let host = api::Host::from_host(host, None, &mut write).await?;

    Ok(api::HostServiceCreateHostResponse {
        host: Some(host),
        token: jwt.into(),
        refresh: encoded.into(),
        provision_org_id: token.org_id.to_string(),
    })
}

pub async fn create_region(
    req: api::HostServiceCreateRegionRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceCreateRegionResponse, Error> {
    let _authz = write.auth(&meta, HostAdminPerm::CreateRegion).await?;

    let new_region = NewRegion {
        key: RegionKey::new(req.region_key.clone())?,
        display_name: &req.display_name,
        sku_code: req.sku_code.as_deref(),
    };
    let region = new_region.create(&mut write).await?;

    Ok(api::HostServiceCreateRegionResponse {
        region: Some(region.into()),
    })
}

pub async fn get_host(
    req: api::HostServiceGetHostRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceGetHostResponse, Error> {
    let id: HostId = req.host_id.parse().map_err(Error::ParseId)?;
    let mut resources = vec![Resource::from(id)];

    let org_id = Host::org_id(id, &mut read).await?;
    let authz = if let Some(org_id) = org_id {
        resources.push(Resource::from(org_id));
        read.auth_or_for(&meta, HostAdminPerm::GetHost, HostPerm::GetHost, &resources)
            .await?
    } else {
        read.auth(&meta, HostAdminPerm::GetHost).await?
    };

    let host = Host::by_id(id, org_id, &mut read).await?;
    let host = api::Host::from_host(host, Some(&authz), &mut read).await?;

    Ok(api::HostServiceGetHostResponse { host: Some(host) })
}

pub async fn get_region(
    req: api::HostServiceGetRegionRequest,
    _meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceGetRegionResponse, Error> {
    // this is a public endpoint that does not need authz

    let region = match req.region.ok_or(Error::MissingRegion)? {
        api::host_service_get_region_request::Region::RegionId(id) => {
            let id = id.parse().map_err(Error::ParseRegionId)?;
            Region::by_id(id, &mut read).await?
        }
        api::host_service_get_region_request::Region::RegionKey(key) => {
            let key = RegionKey::new(key)?;
            Region::by_key(&key, &mut read).await?
        }
    };

    Ok(api::HostServiceGetRegionResponse {
        region: Some(region.into()),
    })
}

pub async fn list_hosts(
    req: api::HostServiceListHostsRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceListHostsResponse, Error> {
    let filter = req.into_filter()?;
    let authz = if filter.org_ids.is_empty() {
        read.auth(&meta, HostAdminPerm::ListHosts).await?
    } else {
        read.auth_or_for(
            &meta,
            HostAdminPerm::ListHosts,
            HostPerm::ListHosts,
            &filter.org_ids,
        )
        .await?
    };

    let (hosts, total) = filter.query(&mut read).await?;
    let hosts = api::Host::from_hosts(hosts, &authz, &mut read).await?;

    Ok(api::HostServiceListHostsResponse { hosts, total })
}

pub async fn list_regions(
    req: api::HostServiceListRegionsRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceListRegionsResponse, Error> {
    let org_id = req
        .org_id
        .as_ref()
        .map(|id| id.parse().map_err(Error::ParseOrgId))
        .transpose()?;

    let authz = if let Some(org_id) = org_id {
        read.auth_or_for(
            &meta,
            HostAdminPerm::ListRegions,
            HostPerm::ListRegions,
            org_id,
        )
        .await?
    } else {
        read.auth(&meta, HostAdminPerm::ListRegions).await?
    };

    let image_id = req.image_id.parse().map_err(Error::ParseImageId)?;
    let image = Image::by_id(image_id, org_id, &authz, &mut read).await?;
    let version =
        ProtocolVersion::by_id(image.protocol_version_id, org_id, &authz, &mut read).await?;
    let protocol = Protocol::by_id(version.protocol_id, org_id, &authz, &mut read).await?;

    let requirements = HostRequirements {
        scheduler: &NodeScheduler::least_resources(),
        protocol: &protocol,
        org_id,
        cpu_cores: image.min_cpu_cores,
        memory_bytes: image.min_memory_bytes,
        disk_bytes: image.min_disk_bytes,
    };

    let mut region_ids = HashSet::new();
    let mut region_hosts = HashMap::new();
    let mut region_ips = HashMap::new();
    let candidates = Host::candidates(requirements, None, &mut read).await?;
    for candidate in candidates {
        let region_id = candidate.host.region_id;
        region_ids.insert(region_id);
        *region_hosts.entry(region_id).or_insert(0) += 1;
        *region_ips.entry(region_id).or_insert(0) += candidate.free_ips;
    }

    let mut regions = Region::by_ids(&region_ids, &mut read).await?;
    regions.sort_by(|r1, r2| r1.key.cmp(&r2.key));
    let regions = regions
        .into_iter()
        .map(|region| {
            let valid_hosts = region_hosts.get(&region.id).copied().unwrap_or(0);
            let free_ips = region_ips.get(&region.id).copied().unwrap_or(0);
            api::RegionInfo {
                region: Some(region.into()),
                valid_hosts,
                free_ips,
            }
        })
        .collect();

    Ok(api::HostServiceListRegionsResponse { regions })
}

pub async fn update_host(
    req: api::HostServiceUpdateHostRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceUpdateHostResponse, Error> {
    let id: HostId = req.host_id.parse().map_err(Error::ParseId)?;
    let mut resources = vec![Resource::from(id)];

    let org_id = Host::org_id(id, &mut write).await?;
    if let Some(org_id) = org_id {
        resources.push(Resource::from(org_id));
    };

    // for public hosts, only a host api token has the update perm
    let authz = if req.cost.is_some() {
        // Only admins can update the cost of a host.
        write
            .auth_for(
                &meta,
                [HostAdminPerm::UpdateHost, HostAdminPerm::ViewCost],
                &resources,
            )
            .await?
    } else {
        write
            .auth_or_for(
                &meta,
                HostAdminPerm::UpdateHost,
                HostPerm::UpdateHost,
                &resources,
            )
            .await?
    };
    let host = Host::by_id(id, org_id, &mut write).await?;

    let region_id = req
        .region_id
        .as_ref()
        .map(|id| id.parse().map_err(Error::ParseRegionId))
        .transpose()?;
    let bv_version = req
        .bv_version
        .as_ref()
        .map(|bv| bv.parse::<Version>().map_err(Error::ParseBvVersion))
        .transpose()?;
    let disk_bytes = req
        .disk_bytes
        .map(|space| space.try_into().map_err(Error::DiskBytes))
        .transpose()?;

    let update = UpdateHost {
        network_name: req.network_name.as_deref(),
        display_name: req.display_name.as_deref(),
        region_id,
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
        cost: req.cost.map(TryInto::try_into).transpose()?,
    };
    let host = update.apply(id, &mut write).await?;
    let host = api::Host::from_host(host, Some(&authz), &mut write).await?;

    Ok(api::HostServiceUpdateHostResponse { host: Some(host) })
}

pub async fn update_region(
    req: api::HostServiceUpdateRegionRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceUpdateRegionResponse, Error> {
    let _authz = write.auth(&meta, HostAdminPerm::UpdateRegion).await?;

    let update = UpdateRegion {
        id: req.region_id.parse().map_err(Error::ParseRegionId)?,
        display_name: req.display_name.as_deref(),
        sku_code: req.sku_code.as_deref(),
    };
    let region = update.apply(&mut write).await?;

    Ok(api::HostServiceUpdateRegionResponse {
        region: Some(region.into()),
    })
}

pub async fn delete_host(
    req: api::HostServiceDeleteHostRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceDeleteHostResponse, Error> {
    let id: HostId = req.host_id.parse().map_err(Error::ParseId)?;
    let mut resources = vec![Resource::from(id)];

    let org_id = Host::org_id(id, &mut write).await?;
    let _authz = if let Some(org_id) = org_id {
        resources.push(Resource::from(org_id));
        write
            .auth_or_for(
                &meta,
                HostAdminPerm::DeleteHost,
                HostPerm::DeleteHost,
                &resources,
            )
            .await?
    } else {
        write.auth(&meta, HostAdminPerm::DeleteHost).await?
    };

    if Node::host_has_nodes(id, &mut write).await? {
        return Err(Error::HasNodes);
    }

    Host::delete(id, org_id, &mut write).await?;
    IpAddress::delete_for_host(id, &mut write).await?;

    Ok(api::HostServiceDeleteHostResponse {})
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

    let command = NewCommand::host(id, CommandType::HostStart)?
        .create(&mut write)
        .await?;
    let command = api::Command::from_host(&command)?.ok_or(Error::NoHostStart)?;
    write.mqtt(command);

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

    let command = NewCommand::host(id, CommandType::HostStop)?
        .create(&mut write)
        .await?;
    let command = api::Command::from_host(&command)?.ok_or(Error::NoHostStop)?;
    write.mqtt(command);

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

    let command = NewCommand::host(id, CommandType::HostRestart)?
        .create(&mut write)
        .await?;
    let command = api::Command::from_host(&command)?.ok_or(Error::NoHostRestart)?;
    write.mqtt(command);

    Ok(api::HostServiceRestartResponse {})
}

impl api::Host {
    pub async fn from_host(
        host: Host,
        authz: Option<&AuthZ>,
        conn: &mut ReadConn<'_, '_>,
    ) -> Result<Self, Error> {
        let lookup = Lookup::from_host(&host, conn).await?;
        Self::from_model(host, &lookup, authz)
    }

    pub async fn from_hosts(
        hosts: Vec<Host>,
        authz: &AuthZ,
        conn: &mut ReadConn<'_, '_>,
    ) -> Result<Vec<Self>, Error> {
        let lookup = Lookup::from_hosts(&hosts, conn).await?;

        hosts
            .into_iter()
            .map(|host| Self::from_model(host, &lookup, Some(authz)))
            .collect()
    }

    fn from_model(host: Host, lookup: &Lookup, authz: Option<&AuthZ>) -> Result<Self, Error> {
        let created_by = host.created_by();
        let org = host.org_id.and_then(|id| lookup.orgs.get(&id));
        let org_name = org.map(|org| org.name.clone());
        let region = lookup
            .regions
            .get(&host.region_id)
            .ok_or(Error::LookupMissingRegion)?;
        let cost = authz.and_then(|authz| common::BillingAmount::from_host(&host, authz));

        let no_ips = vec![];
        let ips = lookup.host_ips.get(&host.id).unwrap_or(&no_ips);
        let assigned = lookup.assigned_ips.get(&host.id).unwrap_or(&no_ips);
        let ip_addresses = ips
            .iter()
            .map(|ip| common::HostIpAddress {
                ip: ip.to_string(),
                assigned: assigned.iter().any(|addr| ip == addr),
            })
            .collect();

        Ok(api::Host {
            host_id: host.id.to_string(),
            org_id: host.org_id.map(|id| id.to_string()),
            org_name,
            region: Some(region.clone().into()),
            network_name: host.network_name,
            display_name: host.display_name,
            schedule_type: common::ScheduleType::from(host.schedule_type).into(),
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
            cost,
        })
    }
}

struct Lookup {
    orgs: HashMap<OrgId, Org>,
    regions: HashMap<RegionId, Region>,
    host_ips: HashMap<HostId, Vec<IpNetwork>>,
    assigned_ips: HashMap<HostId, Vec<IpNetwork>>,
}

impl Lookup {
    async fn from_host(host: &Host, conn: &mut ReadConn<'_, '_>) -> Result<Lookup, Error> {
        Self::from_hosts(std::slice::from_ref(host), conn).await
    }

    async fn from_hosts(hosts: &[Host], conn: &mut ReadConn<'_, '_>) -> Result<Lookup, Error> {
        let host_ids: HashSet<HostId> = hosts.iter().map(|host| host.id).collect();

        let org_ids = hosts.iter().filter_map(|host| host.org_id).collect();
        let orgs = Org::by_ids(&org_ids, conn)
            .await?
            .to_map_keep_last(|org| (org.id, org));

        let region_ids = hosts.iter().map(|host| host.region_id).collect();
        let regions = Region::by_ids(&region_ids, conn)
            .await?
            .to_map_keep_last(|region| (region.id, region));

        let host_ips = IpAddress::for_hosts(&host_ids, conn)
            .await?
            .to_map_keep_all(|ip| (ip.host_id, ip.ip));
        let assigned_ips = IpAddress::assigned_for_hosts(&host_ids, conn)
            .await?
            .to_map_keep_all(|ip| (ip.host_id, ip.ip));

        Ok(Lookup {
            orgs,
            regions,
            host_ips,
            assigned_ips,
        })
    }
}

impl api::HostServiceListHostsRequest {
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
            search,
            sort,
            limit: i64::try_from(self.limit).map_err(Error::FilterLimit)?,
            offset: i64::try_from(self.offset).map_err(Error::FilterOffset)?,
        })
    }
}
