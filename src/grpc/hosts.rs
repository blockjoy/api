use super::api::{self, host_service_server};
use super::helpers;
use crate::auth::expiration_provider;
use crate::{auth, models};
use diesel_async::scoped_futures::ScopedFutureExt;

/// This is a list of all the endpoints that a user is allowed to access with the jwt that they
/// generate on login. It does not contain endpoints like confirm, because those are accessed by a
/// token.
const HOST_ENDPOINTS: [auth::Endpoint; 9] = [
    auth::Endpoint::AuthRefresh,
    auth::Endpoint::BlockchainAll,
    auth::Endpoint::CommandAll,
    auth::Endpoint::DiscoveryAll,
    auth::Endpoint::HostGet,
    auth::Endpoint::HostList,
    auth::Endpoint::KeyFileAll,
    auth::Endpoint::MetricsAll,
    auth::Endpoint::NodeAll,
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

    async fn provision(
        &self,
        req: tonic::Request<api::HostServiceProvisionRequest>,
    ) -> super::Resp<api::HostServiceProvisionResponse> {
        self.trx(|c| provision(req, c).scope_boxed()).await
    }
}

async fn create(
    req: tonic::Request<api::HostServiceCreateRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::HostServiceCreateResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::HostCreate, conn).await?;
    let req = req.into_inner();
    let org_id = req.org_id.as_ref().map(|id| id.parse()).transpose()?;
    let is_allowed = match claims.resource() {
        auth::Resource::User(user_id) => {
            if let Some(org_id) = org_id {
                models::Org::is_member(user_id, org_id, conn).await?
            } else {
                false
            }
        }
        auth::Resource::Org(org) => org_id == Some(org),
        auth::Resource::Host(_) => false,
        auth::Resource::Node(_) => false,
    };
    if !is_allowed {
        super::unauth!("Access denied");
    }
    let new_host = req.as_new()?;
    let host = new_host.create(conn).await?;
    let host = api::Host::from_model(host).await?;
    let resp = api::HostServiceCreateResponse { host: Some(host) };
    Ok(tonic::Response::new(resp))
}

/// Get a host by id.
async fn get(
    req: tonic::Request<api::HostServiceGetRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::HostServiceGetResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::HostGet, conn).await?;
    let req = req.into_inner();
    let host_id = req.id.parse()?;
    let host = models::Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        auth::Resource::User(user_id) => {
            if let Some(org_id) = host.org_id {
                models::Org::is_member(user_id, org_id, conn).await?
            } else {
                false
            }
        }
        auth::Resource::Org(org) => host.org_id == Some(org),
        auth::Resource::Host(host_id) => host.id == host_id,
        auth::Resource::Node(node_id) => {
            models::Node::find_by_id(node_id, conn).await?.host_id == host.id
        }
    };
    if !is_allowed {
        super::unauth!("Access denied");
    }
    let host = api::Host::from_model(host).await?;
    let resp = api::HostServiceGetResponse { host: Some(host) };
    Ok(tonic::Response::new(resp))
}

async fn list(
    req: tonic::Request<api::HostServiceListRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::HostServiceListResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::HostList, conn).await?;
    let req = req.into_inner();
    let org_id = req.org_id.parse()?;
    let is_allowed = match claims.resource() {
        auth::Resource::User(user_id) => models::Org::is_member(user_id, org_id, conn).await?,
        auth::Resource::Org(org_id_) => org_id == org_id_,
        auth::Resource::Host(_) => false,
        auth::Resource::Node(_) => false,
    };
    if !is_allowed {
        super::unauth!("Access denied");
    }
    let hosts = models::Host::filter(org_id, None, conn).await?;
    let hosts = api::Host::from_models(hosts).await?;
    let resp = api::HostServiceListResponse { hosts };
    Ok(tonic::Response::new(resp))
}

async fn update(
    req: tonic::Request<api::HostServiceUpdateRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::HostServiceUpdateResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::HostUpdate, conn).await?;
    let req = req.into_inner();
    let host_id = req.id.parse()?;
    let host = models::Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        auth::Resource::User(user_id) => {
            if let Some(org_id) = host.org_id {
                models::Org::is_member(user_id, org_id, conn).await?
            } else {
                false
            }
        }
        auth::Resource::Org(org_id) => Some(org_id) == host.org_id,
        auth::Resource::Host(host_id) => host_id == host.id,
        auth::Resource::Node(_) => false,
    };
    if !is_allowed {
        super::unauth!("Not allowed to delete host {host_id}!");
    }
    let updater = req.as_update()?;
    updater.update(conn).await?;
    let resp = api::HostServiceUpdateResponse {};
    Ok(tonic::Response::new(resp))
}

async fn delete(
    req: tonic::Request<api::HostServiceDeleteRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::HostServiceDeleteResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::HostDelete, conn).await?;
    let req = req.into_inner();
    let host_id = req.id.parse()?;
    let host = models::Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        auth::Resource::User(user_id) => {
            if let Some(org_id) = host.org_id {
                models::Org::is_member(user_id, org_id, conn).await?
            } else {
                false
            }
        }
        auth::Resource::Org(org_id) => Some(org_id) == host.org_id,
        auth::Resource::Host(host_id) => host_id == host.id,
        auth::Resource::Node(_) => false,
    };
    if !is_allowed {
        super::unauth!("Not allowed to delete host {host_id}!");
    }
    models::Host::delete(host_id, conn).await?;
    let resp = api::HostServiceDeleteResponse {};

    Ok(tonic::Response::new(resp))
}

async fn provision(
    req: tonic::Request<api::HostServiceProvisionRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::HostServiceProvisionResponse> {
    // This endpoint does not have any auth checking in it. This is because the access here is
    // granted using the OTP of the request.
    let req = req.into_inner();
    let provision = models::HostProvision::find_by_id(&req.otp, conn).await?;
    if provision.is_claimed() {
        return Err(tonic::Status::failed_precondition("Provision is already claimed").into());
    }
    let new_host = req.as_new(provision)?;
    let host = models::HostProvision::claim(&req.otp, new_host, conn).await?;
    let iat = chrono::Utc::now();
    let exp = expiration_provider::ExpirationProvider::expiration("TOKEN_EXPIRATION_MINS")?;
    let claims = auth::Claims {
        resource_type: auth::ResourceType::Host,
        resource_id: host.id,
        iat,
        exp: iat + exp,
        endpoints: HOST_ENDPOINTS.iter().copied().collect(),
        data: Default::default(),
    };
    let jwt = auth::Jwt { claims };
    let refresh_exp =
        expiration_provider::ExpirationProvider::expiration("REFRESH_EXPIRATION_HOST_MINS")?;
    let refresh = auth::Refresh::new(host.id, iat, refresh_exp)?;
    let resp = api::HostServiceProvisionResponse {
        host_id: host.id.to_string(),
        token: jwt.encode()?,
        refresh: refresh.encode()?,
    };
    Ok(tonic::Response::new(resp))
}

impl api::Host {
    pub async fn from_models(models: Vec<models::Host>) -> crate::Result<Vec<Self>> {
        models
            .into_iter()
            .map(|model| {
                let mut dto = Self {
                    id: model.id.to_string(),
                    name: model.name,
                    version: model.version,
                    cpu_count: Some(model.cpu_count.try_into()?),
                    mem_size_bytes: Some(model.mem_size_bytes.try_into()?),
                    disk_size_bytes: Some(model.disk_size_bytes.try_into()?),
                    os: model.os,
                    os_version: model.os_version,
                    ip: model.ip_addr,
                    status: 0, // We use the setter to set this field for type-safety
                    created_at: Some(super::try_dt_to_ts(model.created_at)?),
                    ip_range_from: Some(model.ip_range_from.ip().to_string()),
                    ip_range_to: Some(model.ip_range_to.ip().to_string()),
                    ip_gateway: Some(model.ip_gateway.ip().to_string()),
                    org_id: model.org_id.map(|org_id| org_id.to_string()),
                };
                dto.set_status(api::HostStatus::from_model(model.status));
                Ok(dto)
            })
            .collect()
    }

    pub async fn from_model(model: models::Host) -> crate::Result<Self> {
        Ok(Self::from_models(vec![model]).await?[0].clone())
    }
}

impl api::HostServiceCreateRequest {
    pub fn as_new(&self) -> crate::Result<models::NewHost<'_>> {
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
            org_id: self.org_id.as_ref().map(|s| s.parse()).transpose()?,
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

impl api::HostServiceProvisionRequest {
    pub fn as_new(&self, provision: models::HostProvision) -> crate::Result<models::NewHost<'_>> {
        let new_host = models::NewHost {
            name: &self.name,
            version: &self.version,
            cpu_count: self.cpu_count.try_into()?,
            mem_size_bytes: self.mem_size_bytes.try_into()?,
            disk_size_bytes: self.disk_size_bytes.try_into()?,
            os: &self.os,
            os_version: &self.os_version,
            ip_addr: &self.ip,
            status: self.status().into_model()?,
            ip_range_from: provision
                .ip_range_from
                .ok_or_else(helpers::required("provision.ip_range_from"))?,
            ip_range_to: provision
                .ip_range_to
                .ok_or_else(helpers::required("provision.ip_range_to"))?,
            ip_gateway: provision
                .ip_gateway
                .ok_or_else(helpers::required("provision.ip_gateway"))?,
            org_id: provision.org_id,
        };
        Ok(new_host)
    }
}

impl api::HostStatus {
    fn from_model(_model: models::ConnectionStatus) -> Self {
        // todo
        Self::Unspecified
    }
}

impl api::HostConnectionStatus {
    fn _from_model(model: models::ConnectionStatus) -> Self {
        match model {
            models::ConnectionStatus::Online => Self::Online,
            models::ConnectionStatus::Offline => Self::Offline,
        }
    }

    fn into_model(self) -> crate::Result<models::ConnectionStatus> {
        match self {
            Self::Unspecified => Err(crate::Error::unexpected("Unspecified ConnectionStatus")),
            Self::Online => Ok(models::ConnectionStatus::Online),
            Self::Offline => Ok(models::ConnectionStatus::Offline),
        }
    }
}
