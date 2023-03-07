use super::blockjoy_ui::ResponseMeta;
use super::convert;
use crate::auth::{FindableById, HostAuthToken, JwtToken, TokenType, UserAuthToken};
use crate::errors::{self, ApiError};
use crate::grpc::blockjoy_ui::host_service_server::HostService;
use crate::grpc::blockjoy_ui::{
    self, get_hosts_request, CreateHostRequest, CreateHostResponse, DeleteHostRequest,
    DeleteHostResponse, GetHostsRequest, GetHostsResponse, UpdateHostRequest, UpdateHostResponse,
};
use crate::grpc::helpers::{required, try_get_token};
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};

pub struct HostServiceImpl {
    db: models::DbPool,
}

impl HostServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

impl blockjoy_ui::Host {
    pub async fn from_model(
        model: models::Host,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> errors::Result<Self> {
        let nodes = models::Node::find_all_by_host(model.id, conn).await?;
        let nodes = blockjoy_ui::Node::from_models(nodes, conn).await?;
        let dto = Self {
            id: Some(model.id.to_string()),
            org_id: None,
            name: Some(model.name),
            version: model.version,
            location: model.location,
            cpu_count: model.cpu_count,
            mem_size: model.mem_size,
            disk_size: model.disk_size,
            os: model.os,
            os_version: model.os_version,
            ip: Some(model.ip_addr),
            status: None,
            nodes,
            created_at: Some(convert::try_dt_to_ts(model.created_at)?),
            ip_range_from: model.ip_range_from.map(|ip| ip.to_string()),
            ip_range_to: model.ip_range_to.map(|ip| ip.to_string()),
            ip_gateway: model.ip_gateway.map(|ip| ip.to_string()),
        };
        Ok(dto)
    }

    pub fn as_new(&self) -> crate::Result<models::NewHost<'_>> {
        Ok(models::NewHost {
            name: self.name.as_deref().ok_or_else(required("host.name"))?,
            version: self.version.as_deref(),
            location: self.location.as_deref(),
            cpu_count: self.cpu_count,
            mem_size: self.mem_size,
            disk_size: self.disk_size,
            os: self.os.as_deref(),
            os_version: self.os_version.as_deref(),
            ip_addr: self.ip.as_deref().ok_or_else(required("host.ip"))?,
            status: models::ConnectionStatus::Online,
            ip_range_from: self
                .ip_range_from
                .as_ref()
                .ok_or_else(required("host.ip_range_from"))?
                .parse()?,

            ip_range_to: self
                .ip_range_to
                .as_ref()
                .ok_or_else(required("host.ip_range_to"))?
                .parse()?,

            ip_gateway: self
                .ip_gateway
                .as_ref()
                .ok_or_else(required("host.ip_gateway"))?
                .parse()?,
        })
    }

    pub fn as_update(&self) -> crate::Result<models::UpdateHost<'_>> {
        Ok(models::UpdateHost {
            id: self.id.as_ref().ok_or_else(required("host.id"))?.parse()?,
            name: self.name.as_deref(),
            version: self.version.as_deref(),
            location: self.location.as_deref(),
            cpu_count: self.cpu_count,
            mem_size: self.mem_size,
            disk_size: self.disk_size,
            os: self.os.as_deref(),
            os_version: self.os_version.as_deref(),
            ip_addr: self.ip.as_deref(),
            status: None,
            ip_range_from: None,
            ip_range_to: None,
            ip_gateway: None,
        })
    }
}

#[tonic::async_trait]
impl HostService for HostServiceImpl {
    /// Get host(s) by one of:
    /// - ID
    /// - Organization ID
    /// - Token
    /// One of those options need to be there
    async fn get(
        &self,
        request: Request<GetHostsRequest>,
    ) -> Result<Response<GetHostsResponse>, Status> {
        use get_hosts_request::Param;

        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let inner = request.into_inner();
        let meta = inner.meta.ok_or_else(required("meta"))?;
        let request_id = meta.id;
        let param = inner.param.ok_or_else(required("param"))?;
        let mut conn = self.db.conn().await?;
        let response_meta =
            ResponseMeta::new(request_id.unwrap_or_default(), Some(token.try_into()?));
        let hosts = match param {
            Param::Id(id) => {
                let host_id = id.parse().map_err(ApiError::from)?;
                let host = models::Host::find_by_id(host_id, &mut conn).await?;
                let host = blockjoy_ui::Host::from_model(host, &mut conn).await?;
                vec![host]
            }
            Param::Token(token) => {
                let token: HostAuthToken =
                    HostAuthToken::from_encoded(&token, TokenType::HostAuth, true)?;
                let host = token.try_get_host(&mut conn).await?;
                let host = blockjoy_ui::Host::from_model(host, &mut conn).await?;
                vec![host]
            }
        };

        if hosts.is_empty() {
            return Err(Status::not_found("No hosts found"));
        }
        let response = GetHostsResponse {
            meta: Some(response_meta),
            hosts,
        };

        response_with_refresh_token(refresh_token, response)
    }

    async fn create(
        &self,
        request: Request<CreateHostRequest>,
    ) -> Result<Response<CreateHostResponse>, Status> {
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        self.db
            .trx(|c| {
                async move {
                    inner
                        .host
                        .ok_or_else(required("host"))?
                        .as_new()?
                        .create(c)
                        .await
                }
                .scope_boxed()
            })
            .await?;
        let response = CreateHostResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
        };

        Ok(Response::new(response))
    }

    async fn update(
        &self,
        request: Request<UpdateHostRequest>,
    ) -> Result<Response<UpdateHostResponse>, Status> {
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        self.db
            .trx(|c| {
                async move {
                    inner
                        .host
                        .ok_or_else(required("host"))?
                        .as_update()?
                        .update(c)
                        .await
                }
                .scope_boxed()
            })
            .await?;
        let response = UpdateHostResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
        };
        Ok(Response::new(response))
    }

    async fn delete(
        &self,
        request: Request<DeleteHostRequest>,
    ) -> Result<Response<DeleteHostResponse>, Status> {
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        let host_id = inner.id.parse().map_err(ApiError::from)?;
        self.db
            .trx(|c| models::Host::delete(host_id, c).scope_boxed())
            .await?;
        let response = DeleteHostResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
        };

        Ok(Response::new(response))
    }
}
