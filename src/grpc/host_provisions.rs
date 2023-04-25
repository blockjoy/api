use super::api::{self, host_provisions_server};
use super::helpers::required;
use crate::models;
use crate::Result;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response};

#[tonic::async_trait]
impl host_provisions_server::HostProvisions for super::GrpcImpl {
    async fn get(
        &self,
        request: Request<api::GetHostProvisionRequest>,
    ) -> super::Result<api::GetHostProvisionResponse> {
        let request = request.into_inner();
        let host_provision_id = request.id;
        let mut conn = self.conn().await?;
        let host_provision =
            models::HostProvision::find_by_id(&host_provision_id, &mut conn).await?;
        let response = api::GetHostProvisionResponse {
            host_provisions: Some(api::HostProvision::from_model(host_provision)?),
        };
        Ok(Response::new(response))
    }

    async fn create(
        &self,
        request: Request<api::CreateHostProvisionRequest>,
    ) -> super::Result<api::CreateHostProvisionResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let request = request.into_inner();
        let new_provision = request.as_new()?;

        let host_provision = self.trx(|c| new_provision.create(c).scope_boxed()).await?;

        let response = api::CreateHostProvisionResponse {
            host_provision: Some(api::HostProvision::from_model(host_provision)?),
        };

        super::response_with_refresh_token(refresh_token, response)
    }
}

impl api::HostProvision {
    fn from_model(hp: models::HostProvision) -> Result<Self> {
        let install_cmd = hp.install_cmd();
        let hp = Self {
            id: hp.id,
            host_id: hp.host_id.map(|id| id.to_string()),
            org_id: None,
            created_at: Some(super::try_dt_to_ts(hp.created_at)?),
            claimed_at: hp.claimed_at.map(super::try_dt_to_ts).transpose()?,
            install_cmd: Some(install_cmd),
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

impl api::CreateHostProvisionRequest {
    fn as_new(&self) -> crate::Result<models::NewHostProvision> {
        models::NewHostProvision::new(
            None,
            self.ip_range_from.parse()?,
            self.ip_range_to.parse()?,
            self.ip_gateway.parse()?,
            self.org_id.as_ref().map(|id| id.parse()).transpose()?,
        )
    }
}
