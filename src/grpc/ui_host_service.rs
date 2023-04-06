use super::blockjoy_ui::ResponseMeta;
use super::convert;
use crate::auth::{HostAuthToken, JwtToken, TokenType, UserAuthToken};
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

impl blockjoy_ui::Host {
    pub async fn from_model(
        model: models::Host,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> crate::Result<Self> {
        let nodes = models::Node::find_all_by_host(model.id, conn).await?;
        let nodes = blockjoy_ui::Node::from_models(nodes, conn).await?;
        let dto = Self {
            id: model.id.to_string(),
            name: model.name,
            version: model.version,
            location: model.location,
            cpu_count: model.cpu_count.map(|n| n.try_into()).transpose()?,
            mem_size: model.mem_size.map(|n| n.try_into()).transpose()?,
            disk_size: model.disk_size.map(|n| n.try_into()).transpose()?,
            os: model.os,
            os_version: model.os_version,
            ip: model.ip_addr,
            status: model.status.into(),
            nodes,
            created_at: Some(convert::try_dt_to_ts(model.created_at)?),
            ip_range_from: Some(model.ip_range_from.ip().to_string()),
            ip_range_to: Some(model.ip_range_to.ip().to_string()),
            ip_gateway: Some(model.ip_gateway.ip().to_string()),
        };
        Ok(dto)
    }
}

impl blockjoy_ui::CreateHostRequest {
    pub fn as_new(&self) -> crate::Result<models::NewHost<'_>> {
        Ok(models::NewHost {
            name: &self.name,
            version: self.version.as_deref(),
            location: self.location.as_deref(),
            cpu_count: self.cpu_count.map(|n| n.try_into()).transpose()?,
            mem_size: self.mem_size.map(|n| n.try_into()).transpose()?,
            disk_size: self.disk_size.map(|n| n.try_into()).transpose()?,
            os: self.os.as_deref(),
            os_version: self.os_version.as_deref(),
            ip_addr: &self.ip_addr,
            status: models::ConnectionStatus::Online,
            ip_range_from: self.ip_range_from.parse()?,
            ip_range_to: self.ip_range_to.parse()?,
            ip_gateway: self.ip_gateway.parse()?,
        })
    }
}

impl blockjoy_ui::UpdateHostRequest {
    pub fn as_update(&self) -> crate::Result<models::UpdateHost<'_>> {
        Ok(models::UpdateHost {
            id: self.id.parse()?,
            name: self.name.as_deref(),
            version: self.version.as_deref(),
            location: self.location.as_deref(),
            cpu_count: None,
            mem_size: None,
            disk_size: None,
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

#[tonic::async_trait]
impl HostService for super::GrpcImpl {
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
        let mut conn = self.conn().await?;
        let response_meta =
            ResponseMeta::new(request_id.unwrap_or_default(), Some(token.try_into()?));
        let hosts = match param {
            Param::Id(id) => {
                let host_id = id.parse().map_err(crate::Error::from)?;
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
        let new_host = inner.as_new()?;
        self.trx(|c| new_host.create(c).scope_boxed()).await?;
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
        let updater = inner.as_update()?;
        self.trx(|c| updater.update(c).scope_boxed()).await?;
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
        let host_id = inner.id.parse().map_err(crate::Error::from)?;
        self.trx(|c| models::Host::delete(host_id, c).scope_boxed())
            .await?;
        let response = DeleteHostResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
        };

        Ok(Response::new(response))
    }
}
