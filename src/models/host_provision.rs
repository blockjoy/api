use super::schema::host_provisions;
use crate::grpc::{blockjoy, helpers::required};
use crate::Result;
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

#[derive(Debug, Clone, Queryable)]
#[diesel(table_name = host_provisions)]
pub struct HostProvision {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub claimed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub host_id: Option<uuid::Uuid>,
    pub nodes: Option<String>,
    pub ip_range_from: Option<ipnetwork::IpNetwork>,
    pub ip_range_to: Option<ipnetwork::IpNetwork>,
    pub ip_gateway: Option<ipnetwork::IpNetwork>,
}

impl HostProvision {
    pub async fn find_by_id(
        host_provision_id: &str,
        conn: &mut AsyncPgConnection,
    ) -> Result<HostProvision> {
        let host_provision = host_provisions::table
            .find(host_provision_id)
            .get_result(conn)
            .await?;
        // host_provision.set_install_cmd();

        Ok(host_provision)
    }

    /// Wrapper for HostProvision::claim, taking ProvisionHostRequest received via gRPC instead of HostCreateRequest
    pub async fn claim_by_grpc_provision(
        request: &blockjoy::ProvisionHostRequest,
        conn: &mut AsyncPgConnection,
    ) -> Result<super::Host> {
        let new_host = request
            .info
            .as_ref()
            .ok_or_else(required("info"))?
            .as_new()?;
        let prov = HostProvision::claim(&request.otp, new_host, conn).await?;
        Ok(prov)
    }

    pub async fn claim(
        host_provision_id: &str,
        req: super::NewHost<'_>,
        conn: &mut AsyncPgConnection,
    ) -> Result<super::Host> {
        let host_provision = Self::find_by_id(host_provision_id, conn).await?;

        if host_provision.is_claimed() {
            return Err(anyhow::anyhow!("Host provision has already been claimed.").into());
        }

        let host = req.create(conn).await?;

        diesel::update(host_provisions::table.find(host_provision.id))
            .set((
                host_provisions::claimed_at.eq(chrono::Utc::now()),
                host_provisions::host_id.eq(host.id),
            ))
            .execute(conn)
            .await?;

        Ok(host)
    }

    pub fn install_cmd(&self) -> String {
        format!("curl http://bvs.sh | bash -s -- {}", self.id)
    }

    pub fn is_claimed(&self) -> bool {
        self.claimed_at.is_some()
    }
}

/// Since some of the fields here require to be set in a special way, they are left private and the
/// function `new` is offered instead that takes care of setting the fields correctly.
#[derive(Debug, Insertable)]
#[diesel(table_name = host_provisions)]
pub struct NewHostProvision {
    id: String,
    nodes: Option<String>,
    ip_range_from: ipnetwork::IpNetwork,
    ip_range_to: ipnetwork::IpNetwork,
    ip_gateway: ipnetwork::IpNetwork,
}

impl NewHostProvision {
    pub fn new(
        nodes: Option<Vec<super::NodeProvision>>,
        ip_range_from: std::net::IpAddr,
        ip_range_to: std::net::IpAddr,
        ip_gateway: std::net::IpAddr,
    ) -> Result<Self> {
        let nodes = nodes.as_ref().map(serde_json::to_string).transpose()?;
        Ok(Self {
            id: Self::generate_token(),
            nodes,
            ip_range_from: ip_range_from.into(),
            ip_range_to: ip_range_to.into(),
            ip_gateway: ip_gateway.into(),
        })
    }

    pub async fn create(self, conn: &mut AsyncPgConnection) -> Result<HostProvision> {
        let host_provision: HostProvision = diesel::insert_into(host_provisions::table)
            .values(self)
            .get_result(conn)
            .await?;

        Ok(host_provision)
    }

    fn generate_token() -> String {
        use rand::{distributions::Alphanumeric, Rng};
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect()
    }
}
