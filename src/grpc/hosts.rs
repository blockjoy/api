use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;
use futures_util::future::OptionFuture;
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;
use uuid::Uuid;

use crate::auth::claims::{Claims, HostClaims};
use crate::auth::endpoint::Endpoint;
use crate::auth::resource::{HostId, OrgId, Resource};
use crate::auth::token::refresh::Refresh;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::models::command::NewCommand;
use crate::models::host::{HostFilter, MonthlyCostUsd, NewHost, UpdateHost};
use crate::models::{
    Blockchain, CommandType, ConnectionStatus, Host, HostType, Node, Org, OrgUser, Region,
};
use crate::mqtt::Message;
use crate::timestamp::NanosUtc;

use super::api::{self, host_service_server};
use super::common;

/// This is a list of all the endpoints that a user is allowed to access with the jwt that they
/// generate on login. It does not contain endpoints like confirm, because those are accessed by a
/// token.
const HOST_ENDPOINTS: [Endpoint; 14] = [
    Endpoint::AuthRefresh,
    Endpoint::BabelAll,
    Endpoint::BlockchainAll,
    Endpoint::BundleAll,
    Endpoint::CommandAll,
    Endpoint::CookbookAll,
    Endpoint::DiscoveryAll,
    Endpoint::HostGet,
    Endpoint::HostList,
    Endpoint::HostUpdate,
    Endpoint::KeyFileAll,
    Endpoint::ManifestAll,
    Endpoint::MetricsAll,
    Endpoint::NodeAll,
];

#[tonic::async_trait]
impl host_service_server::HostService for super::Grpc {
    async fn create(
        &self,
        req: tonic::Request<api::HostServiceCreateRequest>,
    ) -> super::Resp<api::HostServiceCreateResponse> {
        self.write(|write| create(req, write).scope_boxed()).await
    }

    async fn get(
        &self,
        req: tonic::Request<api::HostServiceGetRequest>,
    ) -> super::Resp<api::HostServiceGetResponse> {
        self.read(|read| get(req, read).scope_boxed()).await
    }

    async fn list(
        &self,
        req: tonic::Request<api::HostServiceListRequest>,
    ) -> super::Resp<api::HostServiceListResponse> {
        self.read(|read| list(req, read).scope_boxed()).await
    }

    async fn update(
        &self,
        req: tonic::Request<api::HostServiceUpdateRequest>,
    ) -> super::Resp<api::HostServiceUpdateResponse> {
        self.write(|write| update(req, write).scope_boxed()).await
    }

    async fn delete(
        &self,
        req: tonic::Request<api::HostServiceDeleteRequest>,
    ) -> super::Resp<api::HostServiceDeleteResponse> {
        self.write(|write| delete(req, write).scope_boxed()).await
    }

    async fn start(
        &self,
        req: tonic::Request<api::HostServiceStartRequest>,
    ) -> super::Resp<api::HostServiceStartResponse> {
        self.write(|write| start(req, write).scope_boxed()).await
    }

    async fn stop(
        &self,
        req: tonic::Request<api::HostServiceStopRequest>,
    ) -> super::Resp<api::HostServiceStopResponse> {
        self.write(|write| stop(req, write).scope_boxed()).await
    }

    async fn restart(
        &self,
        req: tonic::Request<api::HostServiceRestartRequest>,
    ) -> super::Resp<api::HostServiceRestartResponse> {
        self.write(|write| restart(req, write).scope_boxed()).await
    }

    async fn regions(
        &self,
        req: tonic::Request<api::HostServiceRegionsRequest>,
    ) -> super::Resp<api::HostServiceRegionsResponse> {
        self.read(|read| regions(req, read).scope_boxed()).await
    }
}

async fn create(
    req: tonic::Request<api::HostServiceCreateRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::HostServiceCreateResponse> {
    let WriteConn { conn, ctx, .. } = write;
    let req = req.into_inner();
    let org_user = OrgUser::by_token(&req.provision_token, conn)
        .await
        .map_err(|_| tonic::Status::permission_denied("Invalid token"))?;

    if let Some(ref id) = req.org_id {
        // check that the token user is a member of the requested org
        let org_id: OrgId = id.parse()?;
        if org_id != org_user.org_id {
            super::forbidden!("Access denied: not a member of this org");
        }
    }

    let region = req
        .region
        .as_deref()
        .map(|r| Region::get_or_create(r, conn));
    let region = OptionFuture::from(region).await.transpose()?;

    let new_host = req.as_new(org_user, region.as_ref())?;
    let host = new_host.create(conn).await?;

    let resource = Resource::Host(host.id);
    let expires = ctx.config.token.expire.token.try_into()?;
    let claims = Claims::from_now(expires, resource, HOST_ENDPOINTS);
    let token = ctx.auth.cipher.jwt.encode(&claims)?;

    let expires = ctx.config.token.expire.refresh_host.try_into()?;
    let refresh = Refresh::from_now(expires, host.id);
    let encoded = ctx.auth.cipher.refresh.encode(&refresh)?;

    let host = api::Host::from_host(host, &claims, conn).await?;
    let resp = api::HostServiceCreateResponse {
        host: Some(host),
        token: token.into(),
        refresh: encoded.into(),
    };

    Ok(tonic::Response::new(resp))
}

/// Get a host by id.
async fn get(
    req: tonic::Request<api::HostServiceGetRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::HostServiceGetResponse> {
    let ReadConn { conn, ctx } = read;
    let claims = ctx.claims(&req, Endpoint::HostGet, conn).await?;
    let req = req.into_inner();
    let host_id = req.id.parse()?;
    let host = Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => Org::is_member(user_id, host.org_id, conn).await?,
        Resource::Org(org) => host.org_id == org,
        Resource::Host(host_id) => host.id == host_id,
        Resource::Node(node_id) => Node::find_by_id(node_id, conn).await?.host_id == host.id,
    };
    if !is_allowed {
        super::forbidden!("Access denied for hosts get of {}", req.id);
    }
    let host = api::Host::from_host(host, &claims, conn).await?;
    let resp = api::HostServiceGetResponse { host: Some(host) };
    Ok(tonic::Response::new(resp))
}

async fn list(
    req: tonic::Request<api::HostServiceListRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::HostServiceListResponse> {
    let ReadConn { conn, ctx } = read;
    let claims = ctx.claims(&req, Endpoint::HostList, conn).await?;
    let req = req.into_inner();
    let org_id = req.org_id.parse()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => Org::is_member(user_id, org_id, conn).await?,
        Resource::Org(org_id_) => org_id == org_id_,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for hosts list");
    }
    let (host_count, hosts) = Host::filter(req.as_filter()?, conn).await?;
    let hosts = api::Host::from_hosts(hosts, &claims, conn).await?;
    let resp = api::HostServiceListResponse { hosts, host_count };
    Ok(tonic::Response::new(resp))
}

async fn update(
    req: tonic::Request<api::HostServiceUpdateRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::HostServiceUpdateResponse> {
    let WriteConn { conn, ctx, .. } = write;
    let claims = ctx.claims(&req, Endpoint::HostUpdate, conn).await?;
    let req = req.into_inner();
    let host_id = req.id.parse()?;
    let host = Host::find_by_id(host_id, conn).await?;
    if !matches!(claims.resource(), Resource::Host(host_id) if host.id == host_id) {
        super::forbidden!("Access denied for hosts update");
    }
    let region = req.region.as_ref().map(|r| Region::get_or_create(r, conn));
    let region = OptionFuture::from(region).await.transpose()?;
    let updater = req.as_update(region.as_ref())?;
    updater.update(conn).await?;
    let resp = api::HostServiceUpdateResponse {};
    Ok(tonic::Response::new(resp))
}

async fn delete(
    req: tonic::Request<api::HostServiceDeleteRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::HostServiceDeleteResponse> {
    let WriteConn { conn, ctx, .. } = write;
    let claims = ctx.claims(&req, Endpoint::HostDelete, conn).await?;
    let req = req.into_inner();
    let host_id = req.id.parse()?;
    let host = Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => Org::is_member(user_id, host.org_id, conn).await?,
        Resource::Org(org_id) => org_id == host.org_id,
        Resource::Host(host_id) => host_id == host.id,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for hosts delete of {}", req.id);
    }
    Host::delete(host_id, conn).await?;
    let resp = api::HostServiceDeleteResponse {};

    Ok(tonic::Response::new(resp))
}

async fn start(
    req: tonic::Request<api::HostServiceStartRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::HostServiceStartResponse> {
    let WriteConn { conn, ctx, mqtt_tx } = write;
    let claims = ctx.claims(&req, Endpoint::HostStart, conn).await?;
    change_host_state(
        &req.into_inner().id,
        CommandType::RestartBVS,
        claims,
        conn,
        mqtt_tx,
    )
    .await
}

async fn stop(
    req: tonic::Request<api::HostServiceStopRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::HostServiceStopResponse> {
    let WriteConn { conn, ctx, mqtt_tx } = write;
    let claims = ctx.claims(&req, Endpoint::HostStop, conn).await?;
    change_host_state(
        &req.into_inner().id,
        CommandType::StopBVS,
        claims,
        conn,
        mqtt_tx,
    )
    .await
}

async fn restart(
    req: tonic::Request<api::HostServiceRestartRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::HostServiceRestartResponse> {
    let WriteConn { conn, ctx, mqtt_tx } = write;
    let claims = ctx.claims(&req, Endpoint::HostRestart, conn).await?;
    change_host_state(
        &req.into_inner().id,
        CommandType::RestartBVS,
        claims,
        conn,
        mqtt_tx,
    )
    .await
}

async fn change_host_state<Res: Default>(
    id: &str,
    cmd_type: CommandType,
    claims: Claims,
    conn: &mut Conn<'_>,
    mqtt_tx: UnboundedSender<Message>,
) -> super::Result<Res> {
    let host_id = id.parse()?;
    let host = Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => Org::is_member(user_id, host.org_id, conn).await?,
        Resource::Org(org_id) => org_id == host.org_id,
        Resource::Host(host_id) => host_id == host.id,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access not allowed")
    }

    let msg = host_cmd(host_id, cmd_type, conn).await?;
    mqtt_tx.send(msg.into()).expect("mqtt_rx");

    Ok(tonic::Response::new(Default::default()))
}

async fn host_cmd(
    host_id: HostId,
    command_type: CommandType,
    conn: &mut Conn<'_>,
) -> crate::Result<api::Command> {
    let command = NewCommand {
        host_id,
        cmd: command_type,
        sub_cmd: None,
        node_id: None,
    };
    let command = command.create(conn).await?;
    api::Command::from_model(&command, conn).await
}

async fn regions(
    req: tonic::Request<api::HostServiceRegionsRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::HostServiceRegionsResponse> {
    let ReadConn { conn, ctx } = read;
    let claims = ctx.claims(&req, Endpoint::HostRegions, conn).await?;
    let req = req.into_inner();
    let org_id = req.org_id.parse()?;
    let org = Org::find_by_id(org_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => Org::is_member(user_id, org_id, conn).await?,
        Resource::Org(org_id) => org_id == org.id,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access not allowed")
    }
    let host_type = req.host_type().into_model();
    let blockchain = Blockchain::find_by_id(req.blockchain_id.parse()?, conn).await?;
    let node_type = req.node_type().into_model();
    let requirements = ctx
        .cookbook
        .rhai_metadata(&blockchain.name, node_type, &req.version)
        .await?
        .requirements;
    let regions = Host::regions_for(org_id, blockchain, node_type, requirements, host_type, conn)
        .await?
        .into_iter()
        .map(|r| r.name)
        .collect();
    let mut resp = api::HostServiceRegionsResponse { regions };
    resp.regions.sort();
    Ok(tonic::Response::new(resp))
}

impl api::Host {
    pub async fn from_host(
        host: Host,
        claims: &Claims,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Self> {
        let lookup = Lookup::from_host(&host, claims, conn).await?;

        Self::from_model(host, &lookup)
    }

    pub async fn from_hosts(
        hosts: Vec<Host>,
        claims: &Claims,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let lookup = Lookup::from_hosts(&hosts, claims, conn).await?;

        let mut out = Vec::new();
        for host in hosts.into_iter() {
            out.push(Self::from_model(host, &lookup)?);
        }

        Ok(out)
    }

    fn from_model(host: Host, lookup: &Lookup<'_>) -> crate::Result<Self> {
        let billing_amount = lookup
            .claims
            .as_ref()
            .and_then(|claims| claims.get(&host.id))
            .and_then(|claims| common::BillingAmount::from_model(&host, claims));

        Ok(Self {
            id: host.id.to_string(),
            name: host.name,
            version: host.version,
            cpu_count: host.cpu_count.try_into()?,
            mem_size_bytes: host.mem_size_bytes.try_into()?,
            disk_size_bytes: host.disk_size_bytes.try_into()?,
            os: host.os,
            os_version: host.os_version,
            ip: host.ip_addr,
            created_at: Some(NanosUtc::from(host.created_at).into()),
            ip_range_from: host.ip_range_from.ip().to_string(),
            ip_range_to: host.ip_range_to.ip().to_string(),
            ip_gateway: host.ip_gateway.ip().to_string(),
            org_id: host.org_id.to_string(),
            node_count: lookup.nodes.get(&host.id).copied().unwrap_or(0),
            org_name: lookup.orgs[&host.org_id].name.clone(),
            region: host.region_id.map(|id| lookup.regions[&id].name.clone()),
            billing_amount,
            vmm_mountpoint: host.vmm_mountpoint,
        })
    }
}

struct Lookup<'c> {
    claims: Option<HashMap<HostId, HostClaims<'c>>>,
    nodes: HashMap<HostId, u64>,
    orgs: HashMap<OrgId, Org>,
    regions: HashMap<Uuid, Region>,
}

impl<'c> Lookup<'c> {
    async fn from_host(
        host: &Host,
        claims: &'c Claims,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Lookup<'c>> {
        Self::from_hosts(&[host], claims, conn).await
    }

    async fn from_hosts<H>(
        hosts: &[H],
        claims: &'c Claims,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Lookup<'c>>
    where
        H: AsRef<Host>,
    {
        let host_ids = hosts.iter().map(|h| h.as_ref().id).collect();
        let claims = match claims.ensure_hosts(host_ids, false, conn).await {
            Ok(claims) => Some(claims),
            Err(err) => {
                warn!("Failed to ensure claims can view host billing costs: {err}");
                None
            }
        };

        let nodes = Host::node_counts(hosts, conn).await?;

        let org_ids: Vec<_> = hosts.iter().map(|h| h.as_ref().org_id).collect();
        let orgs: HashMap<_, _> = Org::find_by_ids(org_ids, conn)
            .await?
            .into_iter()
            .map(|org| (org.id, org))
            .collect();

        let region_ids = hosts.iter().flat_map(|h| h.as_ref().region_id).collect();
        let regions: HashMap<_, _> = Region::by_ids(region_ids, conn)
            .await?
            .into_iter()
            .map(|region| (region.id, region))
            .collect();

        Ok(Lookup {
            claims,
            nodes,
            orgs,
            regions,
        })
    }
}

impl common::BillingAmount {
    pub fn from_model(host: &Host, claims: &HostClaims<'_>) -> Option<Self> {
        Some(common::BillingAmount {
            amount: Some(common::Amount {
                currency: common::Currency::Usd as i32,
                value: host.monthly_cost_in_usd(claims)?,
            }),
            period: common::Period::Monthly as i32,
        })
    }
}

impl api::HostServiceCreateRequest {
    pub fn as_new(&self, org_user: OrgUser, region: Option<&Region>) -> crate::Result<NewHost<'_>> {
        Ok(NewHost {
            name: &self.name,
            version: &self.version,
            cpu_count: self.cpu_count.try_into()?,
            mem_size_bytes: self.mem_size_bytes.try_into()?,
            disk_size_bytes: self.disk_size_bytes.try_into()?,
            os: &self.os,
            os_version: &self.os_version,
            ip_addr: &self.ip_addr,
            status: ConnectionStatus::Online,
            ip_range_from: self.ip_range_from.parse()?,
            ip_range_to: self.ip_range_to.parse()?,
            ip_gateway: self.ip_gateway.parse()?,
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
    fn as_filter(&self) -> crate::Result<HostFilter> {
        Ok(HostFilter {
            org_id: self.org_id.parse()?,
            offset: self.offset,
            limit: self.limit,
        })
    }
}

impl api::HostServiceUpdateRequest {
    pub fn as_update(&self, region: Option<&Region>) -> crate::Result<UpdateHost<'_>> {
        Ok(UpdateHost {
            id: self.id.parse()?,
            name: self.name.as_deref(),
            version: self.version.as_deref(),
            cpu_count: None,
            mem_size_bytes: None,
            disk_size_bytes: None,
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
    fn into_model(self) -> Option<HostType> {
        match self {
            api::HostType::Unspecified => None,
            api::HostType::Cloud => Some(HostType::Cloud),
            api::HostType::Private => Some(HostType::Private),
        }
    }
}
