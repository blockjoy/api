use std::cmp::max;
use std::collections::{HashMap, HashSet};

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::claims::Claims;
use crate::auth::rbac::{GrpcRole, HostAdminPerm, HostPerm};
use crate::auth::resource::{HostId, OrgId, UserId};
use crate::auth::token::refresh::Refresh;
use crate::auth::{AuthZ, Authorize};
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::model::command::NewCommand;
use crate::model::host::{
    ConnectionStatus, Host, HostFilter, HostSearch, HostSort, HostType, ManagedBy, MonthlyCostUsd,
    NewHost, UpdateHost,
};
use crate::model::{Blockchain, CommandType, IpAddress, Node, Org, Region, RegionId, Token};
use crate::storage::image::ImageId;
use crate::util::{HashVec, NanosUtc};

use super::api::host_service_server::HostService;
use super::{api, common, Grpc, Status};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Host blockchain error: {0}
    Blockchain(#[from] crate::model::blockchain::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Host command error: {0}
    Command(#[from] crate::model::command::Error),
    /// Host command API error: {0}
    CommandApi(#[from] crate::grpc::command::Error),
    /// Failed to parse cpu count: {0}
    CpuCount(std::num::TryFromIntError),
    /// Host provisioning token is not for a user. This should not happen.
    CreateTokenNotUser,
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Failed to parse disk size: {0}
    DiskSize(std::num::TryFromIntError),
    /// This host cannot be deleted because it still has nodes.
    HasNodes,
    /// Host model error: {0}
    Host(#[from] crate::model::host::Error),
    /// Host token error: {0}
    HostProvisionByToken(crate::model::token::Error),
    /// Invalid value for ManagedBy enum: {0}.
    InvalidManagedBy(i32),
    /// Host model error: {0}
    IpAddress(#[from] crate::model::ip_address::Error),
    /// Host JWT failure: {0}
    Jwt(#[from] crate::auth::token::jwt::Error),
    /// Looking is missing org id: {0}
    LookupMissingOrg(OrgId),
    /// Failed to parse mem size: {0}
    MemSize(std::num::TryFromIntError),
    /// Missing org_id for host provisioning token. This should not happen.
    MissingTokenOrgId,
    /// Node model error: {0}
    Node(#[from] crate::model::node::Error),
    /// Host org error: {0}
    Org(#[from] crate::model::org::Error),
    /// Failed to parse BlockchainId: {0}
    ParseBlockchainId(uuid::Error),
    /// Failed to parse HostId: {0}
    ParseId(uuid::Error),
    /// Failed to parse entry from IP's list: {0}
    ParseIp(ipnetwork::IpNetworkError),
    /// Failed to parse IP gateway: {0}
    ParseIpGateway(ipnetwork::IpNetworkError),
    /// Failed to parse non-zero host node_count as u64: {0}
    ParseNodeCount(std::num::TryFromIntError),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Provision token is for a different organization.
    ProvisionOrg,
    /// Host Refresh token failure: {0}
    Refresh(#[from] crate::auth::token::refresh::Error),
    /// Host region error: {0}
    Region(#[from] crate::model::region::Error),
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
            CreateTokenNotUser | Diesel(_) | Jwt(_) | LookupMissingOrg(_) | MissingTokenOrgId
            | ParseNodeCount(_) | Refresh(_) => Status::internal("Internal error."),
            CpuCount(_) | DiskSize(_) | MemSize(_) => Status::out_of_range("Host resource."),
            HasNodes => Status::failed_precondition("This host still has nodes."),
            HostProvisionByToken(_) => Status::forbidden("Invalid token."),
            // HostProvisionByToken(_) => super::Status::forbidden("Invalid token."),
            ParseBlockchainId(_) => Status::invalid_argument("blockchain_id"),
            ParseId(_) => Status::invalid_argument("id"),
            ParseIp(_) => Status::invalid_argument("ips"),
            ParseIpGateway(_) => Status::invalid_argument("ip_gateway"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ProvisionOrg => Status::failed_precondition("Wrong org."),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            InvalidManagedBy(_) => Status::invalid_argument("managed_by"),
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
            Storage(err) => err.into(),
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

async fn create(
    req: api::HostServiceCreateRequest,
    _: super::NaiveMeta,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceCreateResponse, Error> {
    let token = Token::host_provision_by_token(&req.provision_token, &mut write)
        .await
        .map_err(Error::HostProvisionByToken)?;
    let user_id = token.user().ok_or(Error::CreateTokenNotUser)?;
    let org_id = token.org_id.ok_or(Error::MissingTokenOrgId)?;

    if let Some(ref id) = req.org_id {
        let request_org: OrgId = id.parse().map_err(Error::ParseOrgId)?;
        if request_org != org_id {
            return Err(Error::ProvisionOrg);
        }
    }

    let region = if let Some(ref region) = req.region {
        Region::get_or_create(region, None, &mut write)
            .await
            .map(Some)?
    } else {
        None
    };

    let ips: Vec<_> = req
        .ips
        .iter()
        .map(|ip| ip.parse().map_err(Error::ParseIp))
        .collect::<Result<_, _>>()?;
    let host = req
        .as_new(user_id, org_id, region.as_ref())?
        .create(&ips, &mut write)
        .await?;

    let expire_token = write.ctx.config.token.expire.token;
    let expire_refresh = write.ctx.config.token.expire.refresh_host;

    let claims = Claims::from_now(expire_token, host.id, GrpcRole::NewHost);
    let jwt = write.ctx.auth.cipher.jwt.encode(&claims)?;

    let refresh = Refresh::from_now(expire_refresh, host.id);
    let encoded = write.ctx.auth.cipher.refresh.encode(&refresh)?;

    let host = api::Host::from_host(host, None, &mut write).await?;

    Ok(api::HostServiceCreateResponse {
        host: Some(host),
        token: jwt.into(),
        refresh: encoded.into(),
    })
}

async fn get(
    req: api::HostServiceGetRequest,
    meta: super::NaiveMeta,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceGetResponse, Error> {
    let id = req.id.parse().map_err(Error::ParseId)?;
    let authz = read
        .auth_or_all(&meta, HostAdminPerm::Get, HostPerm::Get, id)
        .await?;

    let host = Host::by_id(id, &mut read).await?;
    let host = api::Host::from_host(host, Some(&authz), &mut read).await?;

    Ok(api::HostServiceGetResponse { host: Some(host) })
}

async fn list(
    req: api::HostServiceListRequest,
    meta: super::NaiveMeta,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceListResponse, Error> {
    let filter = req.into_filter()?;
    let authz = if filter.org_ids.is_empty() {
        read.auth_all(&meta, HostAdminPerm::List).await?
    } else {
        read.auth_or_all(&meta, HostAdminPerm::List, HostPerm::List, &filter.org_ids)
            .await?
    };

    let (hosts, host_count) = filter.query(&mut read).await?;
    let hosts = api::Host::from_hosts(hosts, Some(&authz), &mut read).await?;

    Ok(api::HostServiceListResponse { hosts, host_count })
}

async fn update(
    req: api::HostServiceUpdateRequest,
    meta: super::NaiveMeta,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceUpdateResponse, Error> {
    let id: HostId = req.id.parse().map_err(Error::ParseId)?;
    write
        .auth_or_all(&meta, HostAdminPerm::Update, HostPerm::Update, id)
        .await?;
    let host = Host::by_id(id, &mut write).await?;

    let region = if let Some(ref region) = req.region {
        Region::get_or_create(region, None, &mut write)
            .await
            .map(Some)?
    } else {
        None
    };

    req.as_update(&host, region.as_ref())?
        .update(&mut write)
        .await?;

    Ok(api::HostServiceUpdateResponse {})
}

async fn delete(
    req: api::HostServiceDeleteRequest,
    meta: super::NaiveMeta,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceDeleteResponse, Error> {
    let id: HostId = req.id.parse().map_err(Error::ParseId)?;
    write.auth(&meta, HostPerm::Delete, id).await?;

    if !Node::by_host_id(id, &mut write).await?.is_empty() {
        return Err(Error::HasNodes);
    }
    Host::delete(id, &mut write).await?;
    IpAddress::delete_by_host_id(id, &mut write).await?;

    Ok(api::HostServiceDeleteResponse {})
}

async fn start(
    req: api::HostServiceStartRequest,
    meta: super::NaiveMeta,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceStartResponse, Error> {
    let id: HostId = req.id.parse().map_err(Error::ParseId)?;
    let authz = write.auth(&meta, HostPerm::Start, id).await?;

    let _host = Host::by_id(id, &mut write).await?;
    let command = NewCommand::host(id, CommandType::HostStart)?
        .create(&mut write)
        .await?;
    let message = api::Command::from_model(&command, &authz, &mut write).await?;
    write.mqtt(message);

    Ok(api::HostServiceStartResponse {})
}

async fn stop(
    req: api::HostServiceStopRequest,
    meta: super::NaiveMeta,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceStopResponse, Error> {
    let id: HostId = req.id.parse().map_err(Error::ParseId)?;
    let authz = write.auth(&meta, HostPerm::Stop, id).await?;

    let _host = Host::by_id(id, &mut write).await?;
    let command = NewCommand::host(id, CommandType::HostStop)?
        .create(&mut write)
        .await?;
    let message = api::Command::from_model(&command, &authz, &mut write).await?;
    write.mqtt(message);

    Ok(api::HostServiceStopResponse {})
}

async fn restart(
    req: api::HostServiceRestartRequest,
    meta: super::NaiveMeta,
    mut write: WriteConn<'_, '_>,
) -> Result<api::HostServiceRestartResponse, Error> {
    let id: HostId = req.id.parse().map_err(Error::ParseId)?;
    let authz = write.auth(&meta, HostPerm::Restart, id).await?;

    let _host = Host::by_id(id, &mut write).await?;
    let command = NewCommand::host(id, CommandType::HostRestart)?
        .create(&mut write)
        .await?;
    let message = api::Command::from_model(&command, &authz, &mut write).await?;
    write.mqtt(message);

    Ok(api::HostServiceRestartResponse {})
}

async fn regions(
    req: api::HostServiceRegionsRequest,
    meta: super::NaiveMeta,
    mut read: ReadConn<'_, '_>,
) -> Result<api::HostServiceRegionsResponse, Error> {
    let (org_id, authz) = if let Some(org_id) = &req.org_id {
        let org_id: OrgId = org_id.parse().map_err(Error::ParseOrgId)?;
        let authz = read
            .auth_or_all(&meta, HostAdminPerm::Regions, HostPerm::Regions, org_id)
            .await?;
        (Some(org_id), authz)
    } else {
        (None, read.auth_all(&meta, HostAdminPerm::Regions).await?)
    };

    let blockchain_id = req
        .blockchain_id
        .parse()
        .map_err(Error::ParseBlockchainId)?;
    let blockchain = Blockchain::by_id(blockchain_id, &authz, &mut read).await?;

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
        let no_ips = vec![];
        let no_nodes = vec![];
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
            ip_gateway: host.ip_gateway.ip().to_string(),
            org_id: host.org_id.to_string(),
            node_count: u64::try_from(max(0, host.node_count)).map_err(Error::ParseNodeCount)?,
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
                lookup.ip_addresses.get(&host.id).unwrap_or(&no_ips),
                lookup.nodes.get(&host.id).unwrap_or(&no_nodes),
            ),
            managed_by: api::ManagedBy::from_model(host.managed_by).into(),
            tags: Some(host.tags.into_iter().collect()),
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

        let org_ids = hosts.iter().map(|h| h.as_ref().org_id).collect();
        let orgs = Org::by_ids(org_ids, conn)
            .await?
            .to_map_keep_last(|org| (org.id, org));

        let region_ids = hosts.iter().filter_map(|h| h.as_ref().region_id).collect();
        let regions = Region::by_ids(region_ids, conn)
            .await?
            .to_map_keep_last(|region| (region.id, region));

        let ip_addresses = IpAddress::by_host_ids(&host_ids, conn)
            .await?
            .into_iter()
            .filter_map(|ip| ip.host_id.map(|host_id| (host_id, ip)))
            .to_map_keep_all(|(host_id, ip)| (host_id, ip));

        let nodes = Node::by_hosts(&host_ids, conn)
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

impl api::HostIpAddress {
    fn from_models(models: &[IpAddress], nodes: &[Node]) -> Vec<Self> {
        models
            .iter()
            .map(|ip| Self {
                ip: ip.ip().to_string(),
                assigned: nodes.iter().any(|n| n.ip == ip.ip),
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

    pub fn from_stripe(price: &crate::stripe::api::price::Price) -> Option<Self> {
        Some(Self {
            amount: Some(common::Amount {
                currency: price
                    .currency
                    .and_then(common::Currency::from_stripe)
                    .unwrap_or(common::Currency::Usd) as i32,
                value: price.unit_amount?,
            }),
            period: common::Period::Monthly as i32,
        })
    }
}

impl api::HostServiceCreateRequest {
    fn as_new(
        &self,
        user_id: UserId,
        org_id: OrgId,
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
            ip_gateway: self.ip_gateway.parse().map_err(Error::ParseIpGateway)?,
            org_id,
            created_by: user_id,
            region_id: region.map(|r| r.id),
            host_type: HostType::Cloud,
            monthly_cost_in_usd: self
                .billing_amount
                .as_ref()
                .map(MonthlyCostUsd::from_proto)
                .transpose()?,
            vmm_mountpoint: self.vmm_mountpoint.as_deref(),
            managed_by: self
                .managed_by()
                .into_model()
                .ok_or(Error::InvalidManagedBy(self.managed_by.unwrap_or(0)))?,
            tags: self
                .tags
                .as_ref()
                .map(|tags| tags.tags.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|tag| Some(tag.name.trim().to_lowercase()))
                .collect(),
        })
    }
}

impl api::HostServiceListRequest {
    fn into_filter(self) -> Result<HostFilter, Error> {
        let Self {
            org_ids,
            versions,
            offset,
            limit,
            search,
            sort,
        } = self;

        let org_ids = org_ids
            .into_iter()
            .map(|id| id.parse().map_err(Error::ParseOrgId))
            .collect::<Result<_, _>>()?;
        let versions = versions
            .into_iter()
            .map(|v| v.trim().to_lowercase())
            .collect();

        let search = search
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
        let sort = sort
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
                    api::HostSortField::NodeCount => Ok(HostSort::NodeCount(order)),
                }
            })
            .collect::<Result<_, _>>()?;

        Ok(HostFilter {
            org_ids,
            versions,
            offset,
            limit,
            search,
            sort,
        })
    }
}

impl api::HostServiceUpdateRequest {
    pub fn as_update(&self, host: &Host, region: Option<&Region>) -> Result<UpdateHost<'_>, Error> {
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
            ip_gateway: None,
            region_id: region.map(|r| r.id),
            managed_by: self.managed_by().into(),
            tags: self
                .update_tags
                .as_ref()
                .and_then(|ut| ut.as_update(host.tags.iter().flatten())),
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

impl api::ManagedBy {
    const fn from_model(model: ManagedBy) -> Self {
        match model {
            ManagedBy::Automatic => Self::Automatic,
            ManagedBy::Manual => Self::Manual,
        }
    }

    const fn into_model(self) -> Option<ManagedBy> {
        match self {
            Self::Unspecified => None,
            Self::Automatic => Some(ManagedBy::Automatic),
            Self::Manual => Some(ManagedBy::Manual),
        }
    }
}

impl common::UpdateTags {
    pub fn as_update<S: AsRef<str>>(
        &self,
        existing_tags: impl Iterator<Item = S>,
    ) -> Option<Vec<Option<String>>> {
        use common::update_tags::Update;

        match self {
            common::UpdateTags {
                update: Some(Update::OverwriteTags(tags)),
            } => Some(
                tags.tags
                    .iter()
                    .map(|tag| Some(tag.name.trim().to_lowercase()))
                    .collect(),
            ),
            common::UpdateTags {
                update: Some(Update::AddTag(new_tag)),
            } => Some(
                existing_tags
                    .map(|s| s.as_ref().trim().to_lowercase())
                    .chain([new_tag.name.trim().to_lowercase()])
                    .map(Some)
                    .collect(),
            ),
            common::UpdateTags { update: None } => None,
        }
    }
}

impl FromIterator<Option<String>> for common::Tags {
    fn from_iter<T: IntoIterator<Item = Option<String>>>(iter: T) -> Self {
        Self {
            tags: iter
                .into_iter()
                .flatten()
                .map(|name| common::Tag { name })
                .collect(),
        }
    }
}
