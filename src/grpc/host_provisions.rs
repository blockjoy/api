use super::api::{self, host_provision_service_server};
use super::helpers::required;
use crate::Result;
use crate::{auth, models};
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response};

#[tonic::async_trait]
impl host_provision_service_server::HostProvisionService for super::GrpcImpl {
    async fn create(
        &self,
        req: Request<api::HostProvisionServiceCreateRequest>,
    ) -> super::Resp<api::HostProvisionServiceCreateResponse> {
        self.trx(|c| create(req, c).scope_boxed()).await
    }

    async fn get(
        &self,
        req: Request<api::HostProvisionServiceGetRequest>,
    ) -> super::Resp<api::HostProvisionServiceGetResponse> {
        let mut conn = self.conn().await?;
        let resp = get(req, &mut conn).await?;
        Ok(resp)
    }
}

async fn create(
    req: Request<api::HostProvisionServiceCreateRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::HostProvisionServiceCreateResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::HostProvisionCreate, conn).await?;
    let req = req.into_inner();
    let org_id = req.org_id.as_ref().map(|id| id.parse()).transpose()?;
    let is_allowed = is_allowed(claims, org_id, conn).await?;
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let new_provision = req.as_new()?;
    let host_provision = new_provision.create(conn).await?;
    let resp = api::HostProvisionServiceCreateResponse {
        host_provision: Some(api::HostProvision::from_model(host_provision)?),
    };
    Ok(Response::new(resp))
}

async fn get(
    req: Request<api::HostProvisionServiceGetRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::HostProvisionServiceGetResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::HostProvisionGet, conn).await?;
    let req = req.into_inner();
    let host_provision = models::HostProvision::find_by_id(&req.id, conn).await?;
    let is_allowed = is_allowed(claims, host_provision.org_id, conn).await?;
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let resp = api::HostProvisionServiceGetResponse {
        host_provisions: Some(api::HostProvision::from_model(host_provision)?),
    };
    Ok(Response::new(resp))
}

async fn is_allowed(
    claims: auth::Claims,
    org_id: Option<uuid::Uuid>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> crate::Result<bool> {
    match claims.resource() {
        // Users are allowed to manipulate host provisions if they are in the same org as that host
        // provision.
        auth::Resource::User(user_id) => {
            if let Some(org_id) = org_id {
                models::Org::is_member(user_id, org_id, conn).await
            } else {
                Ok(false)
            }
        }
        // Orgs are allowed to manipulate host provisions that belong to that org.
        auth::Resource::Org(org) => Ok(org_id == Some(org)),
        // Hosts and nodes are not allowed access to host provisions.
        auth::Resource::Host(_) | auth::Resource::Node(_) => Ok(false),
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

impl api::HostProvisionServiceCreateRequest {
    fn as_new(&self) -> crate::Result<models::NewHostProvision> {
        models::NewHostProvision::new(
            self.ip_range_from.parse()?,
            self.ip_range_to.parse()?,
            self.ip_gateway.parse()?,
            self.org_id.as_ref().map(|id| id.parse()).transpose()?,
        )
    }
}
