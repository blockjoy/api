use std::collections::{HashMap, HashSet};

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::claims::Claims;
use crate::auth::rbac::{GrpcRole, HostAdminPerm, HostPerm};
use crate::auth::resource::{HostId, OrgId};
use crate::auth::token::refresh::Refresh;
use crate::auth::{AuthZ, Authorize};
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::models::command::NewCommand;
use crate::models::host::{
    ConnectionStatus, Host, HostFilter, HostSearch, HostSort, HostType, MonthlyCostUsd, NewHost,
    UpdateHost,
};
use crate::models::{Blockchain, CommandType, IpAddress, Node, Org, OrgUser, Region, RegionId};
use crate::storage::image::ImageId;
use crate::util::{HashVec, NanosUtc};

use super::api::host_service_server::HostService;
use super::{api, common, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Host blockchain error: {0}
    Blockchain(#[from] crate::models::blockchain::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Host command error: {0}
    Command(#[from] crate::models::command::Error),
    /// Host command API error: {0}
    CommandApi(#[from] crate::grpc::command::Error),
    /// Failed to parse cpu count: {0}
    CpuCount(std::num::TryFromIntError),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Failed to parse disk size: {0}
    DiskSize(std::num::TryFromIntError),
    /// This host cannot be deleted because it still has nodes.
    HasNodes,
    /// Host model error: {0}
    Host(#[from] crate::models::host::Error),
    /// Host model error: {0}
    IpAddress(#[from] crate::models::ip_address::Error),
    /// Host JWT failure: {0}
    Jwt(#[from] crate::auth::token::jwt::Error),
    /// Looking is missing org id: {0}
    LookupMissingOrg(OrgId),
    /// Failed to parse mem size: {0}
    MemSize(std::num::TryFromIntError),
    /// Node model error: {0}
    Node(#[from] crate::models::node::Error),
    /// Host org error: {0}
    Org(#[from] crate::models::org::Error),
    /// Failed to parse BlockchainId: {0}
    ParseBlockchainId(uuid::Error),
    /// Failed to parse HostId: {0}
    ParseId(uuid::Error),
    /// Failed to parse IP from: {0}
    ParseIpFrom(ipnetwork::IpNetworkError),
    /// Failed to parse IP gateway: {0}
    ParseIpGateway(ipnetwork::IpNetworkError),
    /// Failed to parse IP to: {0}
    ParseIpTo(ipnetwork::IpNetworkError),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Provision token is for a different organization.
    ProvisionOrg,
    /// Host Refresh token failure: {0}
    Refresh(#[from] crate::auth::token::refresh::Error),
    /// Host region error: {0}
    Region(#[from] crate::models::region::Error),
    /// Host search failed: {0}
    SearchOperator(crate::util::search::Error),
    /// Sort order: {0}
    SortOrder(crate::util::search::Error),
    /// Host storage error: {0}
    Storage(#[from] crate::storage::Error),
    /// The requested sort field is unknown.
    UnknownSortField,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | Jwt(_) | LookupMissingOrg(_) | Refresh(_) | Storage(_) => {
                Status::internal("Internal error.")
            }
            CpuCount(_) | DiskSize(_) | MemSize(_) => Status::out_of_range("Host resource."),
            HasNodes => Status::failed_precondition("This host still has nodes."),
            ParseBlockchainId(_) => Status::invalid_argument("blockchain_id"),
            ParseId(_) => Status::invalid_argument("id"),
            ParseIpFrom(_) => Status::invalid_argument("ip_range_from"),
            ParseIpGateway(_) => Status::invalid_argument("ip_gateway"),
            ParseIpTo(_) => Status::invalid_argument("ip_range_to"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ProvisionOrg => Status::failed_precondition("Wrong org."),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Blockchain(err) => err.into(),
            Command(err) => err.into(),
            CommandApi(err) => err.into(),
            Host(err) => err.into(),
            IpAddress(err) => err.into(),
            Node(err) => err.into(),
            Org(err) => err.into(),
            Region(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl HostService for Grpc {
    async fn create(
        &self,
        req: Request<api::HostServiceCreateRequest>,
    ) -> Result<Response<api::HostServiceCreateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta, write).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: Request<api::HostServiceGetRequest>,
    ) -> Result<Response<api::HostServiceGetResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta, read).scope_boxed()).await
    }

    async fn list(
        &self,
        req: Request<api::HostServiceListRequest>,
    ) -> Result<Response<api::HostServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }

    async fn update(
        &self,
        req: Request<api::HostServiceUpdateRequest>,
    ) -> Result<Response<api::HostServiceUpdateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta, write).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::HostServiceDeleteRequest>,
    ) -> Result<Response<api::HostServiceDeleteResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta, write).scope_boxed())
            .await
    }

    async fn start(
        &self,
        req: Request<api::HostServiceStartRequest>,
    ) -> Result<Response<api::HostServiceStartResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| start(req, meta, write).scope_boxed())
            .await
    }

    async fn stop(
        &self,
        req: Request<api::HostServiceStopRequest>,
    ) -> Result<Response<api::HostServiceStopResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| stop(req, meta, write).scope_boxed())
            .await
    }

    async fn restart(
        &self,
        req: Request<api::HostServiceRestartRequest>,
    ) -> Result<Response<api::HostServiceRestartResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| restart(req, meta, write).scope_boxed())
            .await
    }

    async fn regions(
        &self,
        req: Request<api::HostServiceRegionsRequest>,
    ) -> Result<Response<api::HostServiceRegionsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| regions(req, meta, read).scope_boxed())
            .await
    }
}

async fn create(
    req: api::HostServiceCreateRequest,
    _meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceCreateResponse, Error> {
    let org_user = OrgUser::by_token(&req.provision_token, &mut write).await?;
    if let Some(ref id) = req.org_id {
        let org_id: OrgId = id.parse().map_err(Error::ParseOrgId)?;
        if org_id != org_user.org_id {
            return Err(Error::ProvisionOrg);
        }
    }

    let region = if let Some(ref region) = req.region {
        Region::get_or_create(region, &mut write).await.map(Some)?
    } else {
        None
    };

    let host = req
        .as_new(&org_user, region.as_ref())?
        .create(&mut write)
        .await?;

    let expire_token = write.ctx.config.token.expire.token;
    let expire_refresh = write.ctx.config.token.expire.refresh_host;

    let claims = Claims::from_now(expire_token, host.id, GrpcRole::NewHost);
    let token = write.ctx.auth.cipher.jwt.encode(&claims)?;

    let refresh = Refresh::from_now(expire_refresh, host.id);
    let encoded = write.ctx.auth.cipher.refresh.encode(&refresh)?;

    let host = api::Host::from_host(host, None, &mut write).await?;

    Ok(api::HostServiceCreateResponse {
        host: Some(host),
        token: token.into(),
        refresh: encoded.into(),
    })
}

async fn get(
    req: api::HostServiceGetRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceGetResponse, Error> {
    let id = req.id.parse().map_err(Error::ParseId)?;
    let authz = read
        .auth_or_all(&meta, HostAdminPerm::Get, HostPerm::Get, id)
        .await?;

    let host = Host::find_by_id(id, &mut read).await?;
    let host = api::Host::from_host(host, Some(&authz), &mut read).await?;

    Ok(api::HostServiceGetResponse { host: Some(host) })
}

async fn list(
    req: api::HostServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceListResponse, Error> {
    let filter = req.into_filter()?;
    let authz = if let Some(org_id) = filter.org_id {
        read.auth_or_all(&meta, HostAdminPerm::List, HostPerm::List, org_id)
            .await?
    } else {
        read.auth_all(&meta, HostAdminPerm::List).await?
    };

    let (hosts, host_count) = filter.query(&mut read).await?;
    let hosts = api::Host::from_hosts(hosts, Some(&authz), &mut read).await?;

    Ok(api::HostServiceListResponse { hosts, host_count })
}

async fn update(
    req: api::HostServiceUpdateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceUpdateResponse, Error> {
    let id: HostId = req.id.parse().map_err(Error::ParseId)?;
    write.auth(&meta, HostPerm::Update, id).await?;

    let region = if let Some(ref region) = req.region {
        Region::get_or_create(region, &mut write).await.map(Some)?
    } else {
        None
    };

    req.as_update(region.as_ref())?.update(&mut write).await?;

    Ok(api::HostServiceUpdateResponse {})
}

async fn delete(
    req: api::HostServiceDeleteRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceDeleteResponse, Error> {
    let id: HostId = req.id.parse().map_err(Error::ParseId)?;
    write.auth(&meta, HostPerm::Delete, id).await?;

    if !Node::find_by_host(id, &mut write).await?.is_empty() {
        return Err(Error::HasNodes);
    }
    Host::delete(id, &mut write).await?;

    Ok(api::HostServiceDeleteResponse {})
}

async fn start(
    req: api::HostServiceStartRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceStartResponse, Error> {
    let id: HostId = req.id.parse().map_err(Error::ParseId)?;
    write.auth(&meta, HostPerm::Start, id).await?;

    let host = Host::find_by_id(id, &mut write).await?;
    let command = NewCommand::host(&host, CommandType::RestartBVS)
        .create(&mut write)
        .await?;
    let message = api::Command::from_model(&command, &mut write).await?;
    write.mqtt(message);

    Ok(api::HostServiceStartResponse {})
}

async fn stop(
    req: api::HostServiceStopRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceStopResponse, Error> {
    let id: HostId = req.id.parse().map_err(Error::ParseId)?;
    write.auth(&meta, HostPerm::Stop, id).await?;

    let host = Host::find_by_id(id, &mut write).await?;
    let command = NewCommand::host(&host, CommandType::StopBVS)
        .create(&mut write)
        .await?;
    let message = api::Command::from_model(&command, &mut write).await?;
    write.mqtt(message);

    Ok(api::HostServiceStopResponse {})
}

async fn restart(
    req: api::HostServiceRestartRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceRestartResponse, Error> {
    let id: HostId = req.id.parse().map_err(Error::ParseId)?;
    write.auth(&meta, HostPerm::Restart, id).await?;

    let host = Host::find_by_id(id, &mut write).await?;
    let command = NewCommand::host(&host, CommandType::RestartBVS)
        .create(&mut write)
        .await?;
    let message = api::Command::from_model(&command, &mut write).await?;
    write.mqtt(message);

    Ok(api::HostServiceRestartResponse {})
}

async fn regions(
    req: api::HostServiceRegionsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceRegionsResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    read.auth(&meta, HostPerm::Regions, org_id).await?;

    let blockchain_id = req
        .blockchain_id
        .parse()
        .map_err(Error::ParseBlockchainId)?;
    let blockchain = Blockchain::find_by_id(blockchain_id, &mut read).await?;

    let node_type = req.node_type().into();
    let host_type = req.host_type().into_model();

    let image = ImageId::new(&blockchain.name, node_type, req.version.into());
    let requirements = read.ctx.storage.rhai_metadata(&image).await?.requirements;

    let mut regions = Host::regions_for(
        org_id,
        blockchain,
        node_type,
        requirements,
        host_type,
        &mut read,
    )
    .await?;
    regions.sort_by(|r1, r2| r1.name.cmp(&r2.name));

    let regions = regions
        .into_iter()
        .map(|r| api::Region {
            name: Some(r.name),
            pricing_tier: r.pricing_tier,
        })
        .collect();

    Ok(api::HostServiceRegionsResponse { regions })
}

impl api::Host {
    pub async fn from_host(
        host: Host,
        authz: Option<&AuthZ>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let lookup = Lookup::from_host(&host, conn).await?;

        Self::from_model(host, authz, &lookup)
    }

    pub async fn from_hosts(
        hosts: Vec<Host>,
        authz: Option<&AuthZ>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let lookup = Lookup::from_hosts(&hosts, conn).await?;

        let mut out = Vec::new();
        for host in hosts {
            out.push(Self::from_model(host, authz, &lookup)?);
        }

        Ok(out)
    }

    fn from_model(host: Host, authz: Option<&AuthZ>, lookup: &Lookup) -> Result<Self, Error> {
        let empty = vec![];
        let billing_amount =
            authz.and_then(|authz| common::BillingAmount::from_model(&host, authz));

        Ok(Self {
            id: host.id.to_string(),
            name: host.name,
            version: host.version,
            cpu_count: host.cpu_count.try_into().map_err(Error::CpuCount)?,
            mem_size_bytes: host.mem_size_bytes.try_into().map_err(Error::MemSize)?,
            disk_size_bytes: host.disk_size_bytes.try_into().map_err(Error::DiskSize)?,
            os: host.os,
            os_version: host.os_version,
            ip: host.ip_addr,
            created_at: Some(NanosUtc::from(host.created_at).into()),
            ip_range_from: host.ip_range_from.ip().to_string(),
            ip_range_to: host.ip_range_to.ip().to_string(),
            ip_gateway: host.ip_gateway.ip().to_string(),
            org_id: host.org_id.to_string(),
            node_count: lookup.nodes.get(&host.id).copied().unwrap_or(0),
            org_name: lookup
                .orgs
                .get(&host.org_id)
                .map(|org| org.name.clone())
                .ok_or(Error::LookupMissingOrg(host.org_id))?,
            region: host
                .region_id
                .and_then(|id| lookup.regions.get(&id).map(|region| region.name.clone())),
            billing_amount,
            vmm_mountpoint: host.vmm_mountpoint,
            ip_addresses: api::HostIpAddress::from_models(
                lookup.ip_addresses.get(&host.id).unwrap_or(&empty),
            ),
        })
    }
}

struct Lookup {
    nodes: HashMap<HostId, u64>,
    orgs: HashMap<OrgId, Org>,
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
        let nodes = Host::node_counts(&host_ids, conn).await?;

        let org_ids = hosts.iter().map(|h| h.as_ref().org_id).collect();
        let orgs = Org::find_by_ids(org_ids, conn)
            .await?
            .to_map_keep_last(|org| (org.id, org));

        let region_ids = hosts.iter().filter_map(|h| h.as_ref().region_id).collect();
        let regions = Region::by_ids(region_ids, conn)
            .await?
            .to_map_keep_last(|region| (region.id, region));

        let ip_addresses = IpAddress::find_by_hosts(host_ids, conn)
            .await?
            .into_iter()
            .filter_map(|ip| ip.host_id.map(|host_id| (host_id, ip)))
            .to_map_keep_all(|(host_id, ip)| (host_id, ip));

        Ok(Lookup {
            nodes,
            orgs,
            regions,
            ip_addresses,
        })
    }
}

impl api::HostIpAddress {
    fn from_models(models: &[IpAddress]) -> Vec<Self> {
        models
            .iter()
            .map(|ip| Self {
                ip: ip.ip().to_string(),
                assigned: ip.is_assigned,
            })
            .collect()
    }
}

impl common::BillingAmount {
    pub fn from_model(host: &Host, authz: &AuthZ) -> Option<Self> {
        Some(common::BillingAmount {
            amount: Some(common::Amount {
                currency: common::Currency::Usd as i32,
                value: host.monthly_cost_in_usd(authz)?,
            }),
            period: common::Period::Monthly as i32,
        })
    }
}

impl api::HostServiceCreateRequest {
    pub fn as_new(
        &self,
        org_user: &OrgUser,
        region: Option<&Region>,
    ) -> Result<NewHost<'_>, Error> {
        Ok(NewHost {
            name: &self.name,
            version: &self.version,
            cpu_count: self.cpu_count.try_into().map_err(Error::CpuCount)?,
            mem_size_bytes: self.mem_size_bytes.try_into().map_err(Error::MemSize)?,
            disk_size_bytes: self.disk_size_bytes.try_into().map_err(Error::DiskSize)?,
            os: &self.os,
            os_version: &self.os_version,
            ip_addr: &self.ip_addr,
            status: ConnectionStatus::Online,
            ip_range_from: self.ip_range_from.parse().map_err(Error::ParseIpFrom)?,
            ip_range_to: self.ip_range_to.parse().map_err(Error::ParseIpTo)?,
            ip_gateway: self.ip_gateway.parse().map_err(Error::ParseIpGateway)?,
            org_id: org_user.org_id,
            created_by: org_user.user_id,
            region_id: region.map(|r| r.id),
            host_type: HostType::Cloud,
            monthly_cost_in_usd: self
                .billing_amount
                .as_ref()
                .map(MonthlyCostUsd::from_proto)
                .transpose()?,
            vmm_mountpoint: self.vmm_mountpoint.as_deref(),
        })
    }
}

impl api::HostServiceListRequest {
    fn into_filter(self) -> Result<HostFilter, Error> {
        let org_id = self
            .org_id
            .map(|id| id.parse().map_err(Error::ParseOrgId))
            .transpose()?;
        let search = self
            .search
            .map(|search| {
                Ok::<_, Error>(HostSearch {
                    operator: search
                        .operator()
                        .try_into()
                        .map_err(Error::SearchOperator)?,
                    id: search.id.map(|id| id.trim().to_lowercase()),
                    name: search.name.map(|name| name.trim().to_lowercase()),
                    version: search.version.map(|version| version.trim().to_lowercase()),
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
                    api::HostSortField::HostName => Ok(HostSort::HostName(order)),
                    api::HostSortField::CreatedAt => Ok(HostSort::CreatedAt(order)),
                    api::HostSortField::Version => Ok(HostSort::Version(order)),
                    api::HostSortField::Os => Ok(HostSort::Os(order)),
                    api::HostSortField::OsVersion => Ok(HostSort::OsVersion(order)),
                    api::HostSortField::CpuCount => Ok(HostSort::CpuCount(order)),
                    api::HostSortField::MemSizeBytes => Ok(HostSort::MemSizeBytes(order)),
                    api::HostSortField::DiskSizeBytes => Ok(HostSort::DiskSizeBytes(order)),
                }
            })
            .collect::<Result<_, _>>()?;

        Ok(HostFilter {
            org_id,
            offset: self.offset,
            limit: self.limit,
            search,
            sort,
        })
    }
}

impl api::HostServiceUpdateRequest {
    pub fn as_update(&self, region: Option<&Region>) -> Result<UpdateHost<'_>, Error> {
        Ok(UpdateHost {
            id: self.id.parse().map_err(Error::ParseId)?,
            name: self.name.as_deref(),
            version: self.version.as_deref(),
            cpu_count: None,
            mem_size_bytes: None,
            disk_size_bytes: self
                .total_disk_space
                .map(|space| space.try_into().map_err(Error::DiskSize))
                .transpose()?,
            os: self.os.as_deref(),
            os_version: self.os_version.as_deref(),
            ip_addr: None,
            status: None,
            ip_range_from: None,
            ip_range_to: None,
            ip_gateway: None,
            region_id: region.map(|r| r.id),
        })
    }
}

impl api::HostType {
    const fn into_model(self) -> Option<HostType> {
        match self {
            api::HostType::Unspecified => None,
            api::HostType::Cloud => Some(HostType::Cloud),
            api::HostType::Private => Some(HostType::Private),
        }
    }
}
