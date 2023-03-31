use super::blockjoy;
use super::helpers::{required, try_get_token};
use crate::auth::{HostAuthToken, JwtToken, TokenRole, TokenType};
use crate::errors::ApiError;
use crate::grpc::blockjoy::host_service_server::HostService;
use crate::grpc::blockjoy::{
    DeleteHostRequest, DeleteHostResponse, HostUpdateRequest, HostUpdateResponse,
    ProvisionHostRequest, ProvisionHostResponse,
};
use crate::models;
use crate::models::{Host, HostProvision};
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};

impl blockjoy::HostUpdateRequest {
    pub fn as_update(&self) -> crate::Result<models::UpdateHost> {
        Ok(models::UpdateHost {
            id: self.id.parse()?,
            name: None,
            version: self.version.as_deref(),
            location: None,
            cpu_count: None,
            mem_size: None,
            disk_size: None,
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

impl blockjoy::ProvisionHostRequest {
    pub fn as_new(&self, provision: models::HostProvision) -> crate::Result<models::NewHost<'_>> {
        let new_host = models::NewHost {
            name: &self.name,
            version: Some(&self.version),
            location: None,
            cpu_count: Some(self.cpu_count),
            mem_size: Some(self.mem_size_bytes),
            disk_size: Some(self.disk_size_bytes),
            os: Some(&self.os),
            os_version: Some(&self.os_version),
            ip_addr: &self.ip,
            status: self.status.try_into()?,
            ip_range_from: provision
                .ip_range_from
                .ok_or_else(required("provision.ip_range_from"))?,
            ip_range_to: provision
                .ip_range_to
                .ok_or_else(required("provision.ip_range_to"))?,
            ip_gateway: provision
                .ip_gateway
                .ok_or_else(required("provision.ip_gateway"))?,
        };
        Ok(new_host)
    }
}

#[tonic::async_trait]
impl HostService for super::GrpcImpl {
    async fn provision(
        &self,
        request: Request<ProvisionHostRequest>,
    ) -> Result<Response<ProvisionHostResponse>, Status> {
        let inner = request.into_inner();
        let request_id = inner.request_id.clone();

        let host = self
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

    async fn update(
        &self,
        request: Request<HostUpdateRequest>,
    ) -> Result<Response<HostUpdateResponse>, Status> {
        let host_token_id = try_get_token::<_, HostAuthToken>(&request)?.id;
        let inner = request.into_inner();
        let request_id = inner.request_id.clone();
        let update_host = inner.as_update()?;

        if host_token_id != update_host.id {
            super::bail_unauthorized!("Not allowed to delete host '{}'", update_host.id);
        }
        self.trx(|c| update_host.update(c).scope_boxed()).await?;
        let result = HostUpdateResponse {
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
        let host_id = inner.host_id.parse().map_err(ApiError::from)?;
        if host_token_id != host_id {
            super::bail_unauthorized!("Not allowed to delete host '{host_id}'");
        }
        self.trx(|c| Host::delete(host_id, c).scope_boxed()).await?;
        let response = DeleteHostResponse {
            messages: vec![],
            origin_request_id: inner.request_id,
        };
        Ok(Response::new(response))
    }
}
