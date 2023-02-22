use super::helpers::{self, try_get_token};
use crate::auth::{HostAuthToken, JwtToken, TokenRole, TokenType};
use crate::errors::ApiError;
use crate::grpc::blockjoy::hosts_server::Hosts;
use crate::grpc::blockjoy::{
    DeleteHostRequest, DeleteHostResponse, HostInfoUpdateRequest, HostInfoUpdateResponse,
    ProvisionHostRequest, ProvisionHostResponse,
};
use crate::grpc::convert::into::IntoData;
use crate::models;
use crate::models::{Host, HostProvision};
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

#[tonic::async_trait]
impl Hosts for HostsServiceImpl {
    async fn provision(
        &self,
        request: Request<ProvisionHostRequest>,
    ) -> Result<Response<ProvisionHostResponse>, Status> {
        let inner = request.into_inner();
        let otp = inner.otp.clone();
        let request_id = inner.request_id.clone();
        let mut tx = self.db.begin().await?;
        let host = HostProvision::claim_by_grpc_provision(&otp, inner, &mut tx)
            .await
            .map_err(|e| match e {
                ApiError::NotFoundError(e) => {
                    Status::not_found(format!("Host provision not found: {e}"))
                }
                _ => Status::internal(format!("Host provision not claimed: {e}")),
            })?;
        tx.commit().await?;
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
        let (request_id, info) = request.into_data()?;
        let request_host_id = info
            .id
            .as_deref()
            .ok_or_else(helpers::required("info.id"))?
            .parse()
            .map_err(ApiError::from)?;
        if host_token_id != request_host_id {
            let msg = format!("Not allowed to delete host '{request_host_id}'");
            return Err(Status::permission_denied(msg));
        }
        let mut tx = self.db.begin().await?;
        Host::update_all(info.try_into()?, &mut tx)
            .await
            .map_err(|e| Status::not_found(format!("Host {request_host_id} not found. {e}")))?;
        tx.commit().await?;
        let result = HostInfoUpdateResponse {
            messages: vec![],
            origin_request_id: Some(request_id),
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
        let mut tx = self.db.begin().await?;
        Host::delete(host_id, &mut tx).await?;
        tx.commit().await?;
        let response = DeleteHostResponse {
            messages: vec![],
            origin_request_id: inner.request_id,
        };
        Ok(Response::new(response))
    }
}
