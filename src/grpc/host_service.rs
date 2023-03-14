use super::blockjoy;
use super::helpers::{required, try_get_token};
use crate::auth::{HostAuthToken, JwtToken, TokenRole, TokenType};
use crate::errors::ApiError;
use crate::grpc::blockjoy::hosts_server::Hosts;
use crate::grpc::blockjoy::{
    DeleteHostRequest, DeleteHostResponse, HostInfoUpdateRequest, HostInfoUpdateResponse,
    ProvisionHostRequest, ProvisionHostResponse,
};
use crate::models;
use crate::models::{Host, HostProvision};
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct HostsServiceImpl {
    db: models::DbPool,
}

impl HostsServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

impl blockjoy::HostInfo {
    pub fn as_update(&self) -> crate::Result<models::UpdateHost> {
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
            status: Some(models::ConnectionStatus::Online),
            ip_range_from: self
                .ip_range_from
                .as_ref()
                .map(|ip| ip.parse())
                .transpose()?,
            ip_range_to: self.ip_range_to.as_ref().map(|ip| ip.parse()).transpose()?,
            ip_gateway: self.ip_gateway.as_ref().map(|ip| ip.parse()).transpose()?,
        })
    }

    pub fn as_new(&self) -> crate::Result<models::NewHost<'_>> {
        let new_host = models::NewHost {
            name: self.name.as_deref().ok_or_else(required("info.name"))?,
            version: self.version.as_deref(),
            location: self.location.as_deref(),
            cpu_count: self.cpu_count,
            mem_size: self.mem_size,
            disk_size: self.disk_size,
            os: self.os.as_deref(),
            os_version: self.os_version.as_deref(),
            ip_addr: self.ip.as_deref().ok_or_else(required("info.ip"))?,
            status: models::ConnectionStatus::Offline,
            ip_range_from: self
                .ip_range_from
                .as_ref()
                .map(|ip| ip.parse())
                .transpose()?,
            ip_range_to: self.ip_range_to.as_ref().map(|ip| ip.parse()).transpose()?,
            ip_gateway: self.ip_gateway.as_ref().map(|ip| ip.parse()).transpose()?,
        };
        Ok(new_host)
    }
}

#[tonic::async_trait]
impl Hosts for HostsServiceImpl {
    async fn provision(
        &self,
        request: Request<ProvisionHostRequest>,
    ) -> Result<Response<ProvisionHostResponse>, Status> {
        let inner = request.into_inner();
        let request_id = inner.request_id.clone();

        let host = self
            .db
            .trx(|c| HostProvision::claim_by_grpc_provision(&inner, c).scope_boxed())
            .await?;
        let token: HostAuthToken = JwtToken::create_token_for::<Host>(
            &host,
            TokenType::HostAuth,
            TokenRole::Service,
            None,
        )?;
        let token = token.encode()?;
        let result = ProvisionHostResponse {
            host_id: host.id.to_string(),
            token,
            messages: vec!["All good".into()],
            origin_request_id: request_id,
        };
        Ok(Response::new(result))
    }

    async fn info_update(
        &self,
        request: Request<HostInfoUpdateRequest>,
    ) -> Result<Response<HostInfoUpdateResponse>, Status> {
        let host_token_id = try_get_token::<_, HostAuthToken>(&request)?.id;
        let inner = request.into_inner();
        let request_id = inner.request_id.clone();
        let update_host = inner
            .info
            .as_ref()
            .ok_or_else(required("info"))?
            .as_update()?;

        if host_token_id != update_host.id {
            let msg = format!("Not allowed to delete host '{}'", update_host.id);
            return Err(Status::permission_denied(msg));
        }
        self.db.trx(|c| update_host.update(c).scope_boxed()).await?;
        let result = HostInfoUpdateResponse {
            messages: vec![],
            origin_request_id: request_id,
        };
        Ok(Response::new(result))
    }

    async fn delete(
        &self,
        request: Request<DeleteHostRequest>,
    ) -> Result<Response<DeleteHostResponse>, Status> {
        let host_token_id = try_get_token::<_, HostAuthToken>(&request)?.id;
        let inner = request.into_inner();
        let host_id = Uuid::parse_str(inner.host_id.as_str()).map_err(ApiError::from)?;
        if host_token_id != host_id {
            let msg = format!("Not allowed to delete host '{host_id}'");
            return Err(Status::permission_denied(msg));
        }
        self.db
            .trx(|c| Host::delete(host_id, c).scope_boxed())
            .await?;
        let response = DeleteHostResponse {
            messages: vec![],
            origin_request_id: inner.request_id,
        };
        Ok(Response::new(response))
    }
}
