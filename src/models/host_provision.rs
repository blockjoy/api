use super::schema::host_provisions;
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
    pub ip_range_from: Option<ipnetwork::IpNetwork>,
    pub ip_range_to: Option<ipnetwork::IpNetwork>,
    pub ip_gateway: Option<ipnetwork::IpNetwork>,
    pub org_id: Option<uuid::Uuid>,
}

impl HostProvision {
    /// Find a host provision by its id. The id is the one time password that is sent to the user
    /// when a host provision is created.
    pub async fn find_by_id(
        host_provision_id: &str,
        conn: &mut AsyncPgConnection,
    ) -> Result<HostProvision> {
        let host_provision = host_provisions::table
            .find(host_provision_id)
            .get_result(conn)
            .await?;
        Ok(host_provision)
    }

    pub async fn claim(
        host_provision_id: &str,
        new_host: super::NewHost<'_>,
        conn: &mut AsyncPgConnection,
    ) -> Result<super::Host> {
        let host = new_host.create(conn).await?;

        diesel::update(host_provisions::table.find(host_provision_id))
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
    ip_range_from: ipnetwork::IpNetwork,
    ip_range_to: ipnetwork::IpNetwork,
    ip_gateway: ipnetwork::IpNetwork,
    org_id: Option<uuid::Uuid>,
}

impl NewHostProvision {
    pub fn new(
        ip_range_from: std::net::IpAddr,
        ip_range_to: std::net::IpAddr,
        ip_gateway: std::net::IpAddr,
        org_id: Option<uuid::Uuid>,
    ) -> Result<Self> {
        Ok(Self {
            id: Self::generate_token(),
            ip_range_from: ip_range_from.into(),
            ip_range_to: ip_range_to.into(),
            ip_gateway: ip_gateway.into(),
            org_id,
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
