use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::claims::Claims;
use crate::auth::endpoint::Endpoint;
use crate::auth::resource::{HostId, OrgId, Resource, UserId};
use crate::auth::token::refresh::Refresh;
use crate::config::Context;
use crate::database::{Conn, Transaction};
use crate::models::command::NewCommand;
use crate::models::host::{HostFilter, NewHost, UpdateHost};
use crate::models::{CommandType, ConnectionStatus, Host, Node, Org, OrgUser};
use crate::timestamp::NanosUtc;

use super::api::{self, host_service_server};

/// This is a list of all the endpoints that a user is allowed to access with the jwt that they
/// generate on login. It does not contain endpoints like confirm, because those are accessed by a
/// token.
const HOST_ENDPOINTS: [Endpoint; 13] = [
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
    Endpoint::MetricsAll,
    Endpoint::NodeAll,
];

#[tonic::async_trait]
impl host_service_server::HostService for super::Grpc {
    async fn create(
        &self,
        req: tonic::Request<api::HostServiceCreateRequest>,
    ) -> super::Resp<api::HostServiceCreateResponse> {
        self.context
            .write(|conn, ctx| create(req, conn, ctx).scope_boxed())
            .await
    }

    /// Get a host by id.
    async fn get(
        &self,
        req: tonic::Request<api::HostServiceGetRequest>,
    ) -> super::Resp<api::HostServiceGetResponse> {
        self.context
            .read(|conn, ctx| get(req, conn, ctx).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: tonic::Request<api::HostServiceListRequest>,
    ) -> super::Resp<api::HostServiceListResponse> {
        self.context
            .read(|conn, ctx| list(req, conn, ctx).scope_boxed())
            .await
    }

    async fn update(
        &self,
        req: tonic::Request<api::HostServiceUpdateRequest>,
    ) -> super::Resp<api::HostServiceUpdateResponse> {
        self.context
            .write(|conn, ctx| update(req, conn, ctx).scope_boxed())
            .await
    }

    async fn start(
        &self,
        req: tonic::Request<api::HostServiceStartRequest>,
    ) -> super::Resp<api::HostServiceStartResponse> {
        self.context
            .write(|conn, ctx| start(req, conn, ctx).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }

    async fn stop(
        &self,
        req: tonic::Request<api::HostServiceStopRequest>,
    ) -> super::Resp<api::HostServiceStopResponse> {
        self.context
            .write(|conn, ctx| stop(req, conn, ctx).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }

    async fn restart(
        &self,
        req: tonic::Request<api::HostServiceRestartRequest>,
    ) -> super::Resp<api::HostServiceRestartResponse> {
        self.context
            .write(|conn, ctx| restart(req, conn, ctx).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }

    async fn delete(
        &self,
        req: tonic::Request<api::HostServiceDeleteRequest>,
    ) -> super::Resp<api::HostServiceDeleteResponse> {
        self.context
            .write(|conn, ctx| delete(req, conn, ctx).scope_boxed())
            .await
    }
}

async fn create(
    req: tonic::Request<api::HostServiceCreateRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::HostServiceCreateResponse> {
    let req = req.into_inner();
    let org_id = req.org_id.as_ref().map(|id| id.parse()).transpose()?;
    // We retrieve the id of the caller from the token that was used.
    let (caller_id, org_id) = if let Some(org_id) = org_id {
        // First we find the org and user that correspond to this token.
        let org_user = OrgUser::by_token(&req.provision_token, conn)
            .await
            .map_err(|_| tonic::Status::permission_denied("Invalid token"))?;
        // Now we check that the user belonging to this token is actually a member of the requested
        // organization.
        if org_user.org_id == org_id {
            (org_user.user_id, org_id)
        } else {
            super::forbidden!("Access denied: not a member of this org");
        }
    } else {
        // First we find the org and user that correspond to this token.
        let org_user = OrgUser::by_token(&req.provision_token, conn)
            .await
            .map_err(|_| tonic::Status::permission_denied("Invalid token"))?;
        (org_user.user_id, org_user.org_id)
    };
    let new_host = req.as_new(caller_id, org_id)?;
    let host = new_host.create(conn).await?;

    let resource = Resource::Host(host.id);
    let expires = ctx.config.token.expire.token.try_into()?;
    let claims = Claims::from_now(expires, resource, HOST_ENDPOINTS);
    let token = ctx.cipher().jwt.encode(&claims)?;

    let expires = ctx.config.token.expire.refresh_host.try_into()?;
    let refresh = Refresh::from_now(expires, host.id);
    let encoded = ctx.cipher().refresh.encode(&refresh)?;

    let host = api::Host::from_model(host, conn).await?;
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
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::HostServiceGetResponse> {
    let claims = ctx.claims(&req, Endpoint::HostGet).await?;
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
    let host = api::Host::from_model(host, conn).await?;
    let resp = api::HostServiceGetResponse { host: Some(host) };
    Ok(tonic::Response::new(resp))
}

async fn list(
    req: tonic::Request<api::HostServiceListRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::HostServiceListResponse> {
    let claims = ctx.claims(&req, Endpoint::HostList).await?;
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
    let hosts = api::Host::from_models(hosts, conn).await?;
    let resp = api::HostServiceListResponse { hosts, host_count };
    Ok(tonic::Response::new(resp))
}

async fn update(
    req: tonic::Request<api::HostServiceUpdateRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::HostServiceUpdateResponse> {
    let claims = ctx.claims(&req, Endpoint::HostUpdate).await?;
    let req = req.into_inner();
    let host_id = req.id.parse()?;
    let host = Host::find_by_id(host_id, conn).await?;
    if !matches!(claims.resource(), Resource::Host(host_id) if host.id == host_id) {
        super::forbidden!("Access denied for hosts update");
    }
    let updater = req.as_update()?;
    updater.update(conn).await?;
    let resp = api::HostServiceUpdateResponse {};
    Ok(tonic::Response::new(resp))
}

async fn delete(
    req: tonic::Request<api::HostServiceDeleteRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::HostServiceDeleteResponse> {
    let claims = ctx.claims(&req, Endpoint::HostDelete).await?;
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
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> crate::Result<super::Outcome<api::HostServiceStartResponse>> {
    let claims = ctx.claims(&req, Endpoint::HostStart).await?;
    change_host_state(&req.into_inner().id, CommandType::RestartBVS, claims, conn).await
}

async fn stop(
    req: tonic::Request<api::HostServiceStopRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> crate::Result<super::Outcome<api::HostServiceStopResponse>> {
    let claims = ctx.claims(&req, Endpoint::HostStop).await?;
    change_host_state(&req.into_inner().id, CommandType::StopBVS, claims, conn).await
}

async fn restart(
    req: tonic::Request<api::HostServiceRestartRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> crate::Result<super::Outcome<api::HostServiceRestartResponse>> {
    let claims = ctx.claims(&req, Endpoint::HostRestart).await?;
    change_host_state(&req.into_inner().id, CommandType::RestartBVS, claims, conn).await
}

async fn change_host_state<Res: Default>(
    id: &str,
    cmd_type: CommandType,
    claims: Claims,
    conn: &mut Conn<'_>,
) -> crate::Result<super::Outcome<Res>> {
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
    Ok(super::Outcome::new(Default::default()).with_msg(host_cmd(host_id, cmd_type, conn).await?))
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

impl api::Host {
    pub async fn from_models(models: Vec<Host>, conn: &mut Conn<'_>) -> crate::Result<Vec<Self>> {
        let node_counts = Host::node_counts(&models, conn).await?;

        let org_ids: Vec<_> = models.iter().map(|h| h.org_id).collect();
        let orgs: HashMap<_, _> = Org::find_by_ids(org_ids, conn)
            .await?
            .into_iter()
            .map(|org| (org.id, org))
            .collect();

        models
            .into_iter()
            .map(|model| {
                Ok(Self {
                    id: model.id.to_string(),
                    name: model.name,
                    version: model.version,
                    cpu_count: model.cpu_count.try_into()?,
                    mem_size_bytes: model.mem_size_bytes.try_into()?,
                    disk_size_bytes: model.disk_size_bytes.try_into()?,
                    os: model.os,
                    os_version: model.os_version,
                    ip: model.ip_addr,
                    created_at: Some(NanosUtc::from(model.created_at).into()),
                    ip_range_from: model.ip_range_from.ip().to_string(),
                    ip_range_to: model.ip_range_to.ip().to_string(),
                    ip_gateway: model.ip_gateway.ip().to_string(),
                    org_id: model.org_id.to_string(),
                    node_count: node_counts.get(&model.id).copied().unwrap_or(0),
                    org_name: orgs[&model.org_id].name.clone(),
                })
            })
            .collect()
    }

    pub async fn from_model(model: Host, conn: &mut Conn<'_>) -> crate::Result<Self> {
        Ok(Self::from_models(vec![model], conn).await?[0].clone())
    }
}

impl api::HostServiceCreateRequest {
    pub fn as_new(&self, user_id: UserId, org_id: OrgId) -> crate::Result<NewHost<'_>> {
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
            org_id,
            created_by: user_id,
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
    pub fn as_update(&self) -> crate::Result<UpdateHost<'_>> {
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
        })
    }
}
