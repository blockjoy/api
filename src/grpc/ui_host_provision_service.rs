use super::convert;
use super::helpers::required;
use crate::auth::UserAuthToken;
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy_ui::host_provision_service_server::HostProvisionService;
use crate::grpc::blockjoy_ui::{
    CreateHostProvisionRequest, CreateHostProvisionResponse, GetHostProvisionRequest,
    GetHostProvisionResponse, HostProvision as GrpcHostProvision, ResponseMeta,
};
use crate::grpc::helpers::try_get_token;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::models;
use crate::models::{HostProvision, HostProvisionRequest};
use anyhow::anyhow;
use std::net::AddrParseError;
use tonic::{Request, Response, Status};

pub struct HostProvisionServiceImpl {
    db: models::DbPool,
}

impl HostProvisionServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

impl GrpcHostProvision {
    fn from_model(hp: HostProvision, _conn: &mut sqlx::PgConnection) -> Result<Self> {
        let hp = Self {
            id: Some(hp.id),
            host_id: hp.host_id.map(|id| id.to_string()),
            org_id: None,
            created_at: Some(convert::try_dt_to_ts(hp.created_at)?),
            claimed_at: hp.claimed_at.map(convert::try_dt_to_ts).transpose()?,
            install_cmd: hp.install_cmd.map(String::from),
            ip_range_from: hp
                .ip_range_from
                .map(|ip| ip.to_string())
                .ok_or_else(required("host_provision.ip_range_from"))?,
            ip_range_to: hp
                .ip_range_to
                .map(|ip| ip.to_string())
                .ok_or_else(required("host_provision.ip_range_to"))?,
            ip_gateway: hp
                .ip_gateway
                .map(|ip| ip.to_string())
                .ok_or_else(required("host_provision.ip_gateway"))?,
        };
        Ok(hp)
    }
}

#[tonic::async_trait]
impl HostProvisionService for HostProvisionServiceImpl {
    async fn get(
        &self,
        request: Request<GetHostProvisionRequest>,
    ) -> Result<Response<GetHostProvisionResponse>, Status> {
        let inner = request.into_inner();
        let host_provision_id = inner.id.ok_or_else(required("id"))?;
        let mut conn = self.db.conn().await?;
        let host_provision = HostProvision::find_by_id(&host_provision_id, &mut conn).await?;
        let response = GetHostProvisionResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, None)),
            host_provisions: vec![GrpcHostProvision::from_model(host_provision, &mut conn)?],
        };
        Ok(Response::new(response))
    }

    async fn create(
        &self,
        request: Request<CreateHostProvisionRequest>,
    ) -> Result<Response<CreateHostProvisionResponse>, Status> {
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let refresh_token = get_refresh_token(&request);
        let inner = request.into_inner();
        let provision = inner
            .host_provision
            .ok_or_else(required("host_provision"))?;
        let req = HostProvisionRequest {
            nodes: None,
            ip_range_from: provision
                .ip_range_from
                .parse()
                .map_err(|err: AddrParseError| ApiError::UnexpectedError(anyhow!(err)))?,
            ip_range_to: provision
                .ip_range_to
                .parse()
                .map_err(|err: AddrParseError| ApiError::UnexpectedError(anyhow!(err)))?,
            ip_gateway: provision
                .ip_gateway
                .parse()
                .map_err(|err: AddrParseError| ApiError::UnexpectedError(anyhow!(err)))?,
        };

        let mut tx = self.db.begin().await?;
        let provision = HostProvision::create(req, &mut tx).await?;
        tx.commit().await?;
        let meta = ResponseMeta::from_meta(inner.meta, Some(token)).with_message(provision.id);
        let response = CreateHostProvisionResponse { meta: Some(meta) };

        Ok(response_with_refresh_token(refresh_token, response)?)
    }
}
