use super::blockjoy_ui::ResponseMeta;
use super::convert;
use crate::auth::{HostAuthToken, JwtToken, TokenType};
use crate::errors::{self, ApiError};
use crate::grpc::blockjoy_ui::host_service_server::HostService;
use crate::grpc::blockjoy_ui::{
    self, get_hosts_request, CreateHostRequest, CreateHostResponse, DeleteHostRequest,
    DeleteHostResponse, GetHostsRequest, GetHostsResponse, UpdateHostRequest, UpdateHostResponse,
};
use crate::grpc::helpers::required;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::models::{self, Host, HostRequest, HostSelectiveUpdate};
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
        db: &mut sqlx::PgConnection,
    ) -> errors::Result<Self> {
        let nodes = models::Node::find_all_by_host(model.id, &mut *db).await?;
        let nodes = blockjoy_ui::Node::from_models(nodes, &mut *db).await?;
        let dto = Self {
            id: Some(model.id.to_string()),
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
        let inner = request.into_inner();
        let meta = inner.meta.ok_or_else(required("meta"))?;
        let request_id = meta.id;
        let param = inner.param.ok_or_else(required("param"))?;
        let mut conn = self.db.conn().await?;
        let response_meta = ResponseMeta::new(request_id.unwrap_or_default());
        let hosts = match param {
            Param::Id(id) => {
                let host_id = id.parse().map_err(ApiError::from)?;
                let host = Host::find_by_id(host_id, &mut conn).await?;
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

        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    async fn create(
        &self,
        request: Request<CreateHostRequest>,
    ) -> Result<Response<CreateHostResponse>, Status> {
        let inner = request.into_inner();
        let host = inner.host.ok_or_else(required("host"))?;
        let fields: HostRequest = host.try_into()?;
        let mut tx = self.db.begin().await?;
        Host::create(fields, &mut tx).await?;
        tx.commit().await?;
        let response = CreateHostResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta)),
        };

        Ok(Response::new(response))
    }

    async fn update(
        &self,
        request: Request<UpdateHostRequest>,
    ) -> Result<Response<UpdateHostResponse>, Status> {
        let inner = request.into_inner();
        let host = inner.host.ok_or_else(required("host"))?;
        let fields: HostSelectiveUpdate = host.try_into()?;

        let mut tx = self.db.begin().await?;
        Host::update_all(fields, &mut tx).await?;
        tx.commit().await?;
        let response = UpdateHostResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta)),
        };
        Ok(Response::new(response))
    }

    async fn delete(
        &self,
        request: Request<DeleteHostRequest>,
    ) -> Result<Response<DeleteHostResponse>, Status> {
        let inner = request.into_inner();
        let host_id = inner.id.parse().map_err(ApiError::from)?;
        let mut tx = self.db.begin().await?;
        Host::delete(host_id, &mut tx).await?;
        tx.commit().await?;
        let response = DeleteHostResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta)),
        };

        Ok(Response::new(response))
    }
}
