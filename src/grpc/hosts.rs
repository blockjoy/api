use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;

use super::api::{self, host_service_server};
use crate::auth::token::refresh::Refresh;
use crate::auth::token::{Claims, Endpoint, Resource, ResourceType};
use crate::{auth, models};

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
impl host_service_server::HostService for super::GrpcImpl {
    async fn create(
        &self,
        req: tonic::Request<api::HostServiceCreateRequest>,
    ) -> super::Resp<api::HostServiceCreateResponse> {
        self.trx(|c| create(req, c).scope_boxed()).await
    }

    /// Get a host by id.
    async fn get(
        &self,
        req: tonic::Request<api::HostServiceGetRequest>,
    ) -> super::Resp<api::HostServiceGetResponse> {
        let mut conn = self.conn().await?;
        let resp = get(req, &mut conn).await?;
        Ok(resp)
    }

    async fn list(
        &self,
        req: tonic::Request<api::HostServiceListRequest>,
    ) -> super::Resp<api::HostServiceListResponse> {
        let mut conn = self.conn().await?;
        let resp = list(req, &mut conn).await?;
        Ok(resp)
    }

    async fn update(
        &self,
        req: tonic::Request<api::HostServiceUpdateRequest>,
    ) -> super::Resp<api::HostServiceUpdateResponse> {
        self.trx(|c| update(req, c).scope_boxed()).await
    }

    async fn delete(
        &self,
        req: tonic::Request<api::HostServiceDeleteRequest>,
    ) -> super::Resp<api::HostServiceDeleteResponse> {
        self.trx(|c| delete(req, c).scope_boxed()).await
    }
}

async fn create(
    req: tonic::Request<api::HostServiceCreateRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::HostServiceCreateResponse> {
    let req = req.into_inner();
    let org_id = req.org_id.as_ref().map(|id| id.parse()).transpose()?;
    // We retrieve the id of the caller from the token that was used.
    let (caller_id, org_id) = if let Some(org_id) = org_id {
        // First we find the org and user that correspond to this token.
        let org_user = models::OrgUser::by_token(&req.provision_token, conn)
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
        let org_user = models::OrgUser::by_token(&req.provision_token, conn)
            .await
            .map_err(|_| tonic::Status::permission_denied("Invalid token"))?;
        (org_user.user_id, org_user.org_id)
    };
    let new_host = req.as_new(caller_id, org_id)?;
    let host = new_host.create(conn).await?;
    let iat = chrono::Utc::now();
    let exp = conn.context.config.token.expire.token.try_into()?;
    let claims = Claims::new(
        ResourceType::Host,
        host.id,
        iat,
        exp,
        HOST_ENDPOINTS.iter().copied().collect(),
    )?;

    let refresh_exp = conn.context.config.token.expire.refresh_host.try_into()?;
    let refresh = Refresh::new(host.id, iat, refresh_exp)?;

    let host = api::Host::from_model(host, conn).await?;
    let resp = api::HostServiceCreateResponse {
        host: Some(host),
        token: conn.context.cipher.jwt.encode(&claims)?.into(),
        refresh: conn.context.cipher.refresh.encode(&refresh)?.into(),
    };

    Ok(tonic::Response::new(resp))
}

/// Get a host by id.
async fn get(
    req: tonic::Request<api::HostServiceGetRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::HostServiceGetResponse> {
    let claims = auth::get_claims(&req, Endpoint::HostGet, conn).await?;
    let req = req.into_inner();
    let host_id = req.id.parse()?;
    let host = models::Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, host.org_id, conn).await?,
        Resource::Org(org) => host.org_id == org,
        Resource::Host(host_id) => host.id == host_id,
        Resource::Node(node_id) => {
            models::Node::find_by_id(node_id, conn).await?.host_id == host.id
        }
    };
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let host = api::Host::from_model(host, conn).await?;
    let resp = api::HostServiceGetResponse { host: Some(host) };
    Ok(tonic::Response::new(resp))
}

async fn list(
    req: tonic::Request<api::HostServiceListRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::HostServiceListResponse> {
    let claims = auth::get_claims(&req, Endpoint::HostList, conn).await?;
    let req = req.into_inner();
    let org_id = req.org_id.parse()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, org_id, conn).await?,
        Resource::Org(org_id_) => org_id == org_id_,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let (host_count, hosts) = models::Host::filter(req.as_filter()?, conn).await?;
    let hosts = api::Host::from_models(hosts, conn).await?;
    let resp = api::HostServiceListResponse { hosts, host_count };
    Ok(tonic::Response::new(resp))
}

async fn update(
    req: tonic::Request<api::HostServiceUpdateRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::HostServiceUpdateResponse> {
    let claims = auth::get_claims(&req, Endpoint::HostUpdate, conn).await?;
    let req = req.into_inner();
    let host_id = req.id.parse()?;
    let host = models::Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, host.org_id, conn).await?,
        Resource::Org(org_id) => org_id == host.org_id,
        Resource::Host(host_id) => host_id == host.id,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Not allowed to delete host {host_id}!");
    }
    let updater = req.as_update()?;
    updater.update(conn).await?;
    let resp = api::HostServiceUpdateResponse {};
    Ok(tonic::Response::new(resp))
}

async fn delete(
    req: tonic::Request<api::HostServiceDeleteRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::HostServiceDeleteResponse> {
    let claims = auth::get_claims(&req, Endpoint::HostDelete, conn).await?;
    let req = req.into_inner();
    let host_id = req.id.parse()?;
    let host = models::Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, host.org_id, conn).await?,
        Resource::Org(org_id) => org_id == host.org_id,
        Resource::Host(host_id) => host_id == host.id,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Not allowed to delete host {host_id}!");
    }
    models::Host::delete(host_id, conn).await?;
    let resp = api::HostServiceDeleteResponse {};

    Ok(tonic::Response::new(resp))
}

impl api::Host {
    pub async fn from_models(
        models: Vec<models::Host>,
        conn: &mut models::Conn,
    ) -> crate::Result<Vec<Self>> {
        let node_counts = models::Host::node_counts(&models, conn).await?;

        let org_ids: Vec<_> = models.iter().map(|h| h.org_id).collect();
        let orgs: HashMap<_, _> = models::Org::find_by_ids(org_ids, conn)
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
                    created_at: Some(super::try_dt_to_ts(model.created_at)?),
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

    pub async fn from_model(model: models::Host, conn: &mut models::Conn) -> crate::Result<Self> {
        Ok(Self::from_models(vec![model], conn).await?[0].clone())
    }
}

impl api::HostServiceCreateRequest {
    pub fn as_new(
        &self,
        user_id: uuid::Uuid,
        org_id: uuid::Uuid,
    ) -> crate::Result<models::NewHost<'_>> {
        Ok(models::NewHost {
            name: &self.name,
            version: &self.version,
            cpu_count: self.cpu_count.try_into()?,
            mem_size_bytes: self.mem_size_bytes.try_into()?,
            disk_size_bytes: self.disk_size_bytes.try_into()?,
            os: &self.os,
            os_version: &self.os_version,
            ip_addr: &self.ip_addr,
            status: models::ConnectionStatus::Online,
            ip_range_from: self.ip_range_from.parse()?,
            ip_range_to: self.ip_range_to.parse()?,
            ip_gateway: self.ip_gateway.parse()?,
            org_id,
            created_by: user_id,
        })
    }
}

impl api::HostServiceListRequest {
    fn as_filter(&self) -> crate::Result<models::HostFilter> {
        Ok(models::HostFilter {
            org_id: self.org_id.parse()?,
            offset: self.offset,
            limit: self.limit,
        })
    }
}

impl api::HostServiceUpdateRequest {
    pub fn as_update(&self) -> crate::Result<models::UpdateHost<'_>> {
        Ok(models::UpdateHost {
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
