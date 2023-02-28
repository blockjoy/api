use super::schema::{hosts, nodes};
use crate::auth::{FindableById, HostAuthToken, Identifiable, JwtToken, Owned, TokenError};
use crate::cookbook::HardwareRequirements;
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy;
use crate::grpc::helpers::required;
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumConnStatus"]
pub enum ConnectionStatus {
    Online,
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumHostType"]
pub enum HostType {
    Cloud,
    Enterprise,
}

#[derive(Clone, Queryable)]
#[diesel(table_name = hosts)]
pub struct Host {
    pub id: Uuid,
    pub version: Option<String>,
    pub name: String,
    pub location: Option<String>,
    pub ip_addr: String,
    pub status: ConnectionStatus,
    pub created_at: DateTime<Utc>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub ip_range_from: Option<ipnetwork::IpNetwork>,
    pub ip_range_to: Option<ipnetwork::IpNetwork>,
    pub ip_gateway: Option<ipnetwork::IpNetwork>,
    pub used_cpu: Option<i32>,
    pub used_memory: Option<i64>,
    pub used_disk_space: Option<i64>,
    pub load_one: Option<f64>,
    pub load_five: Option<f64>,
    pub load_fifteen: Option<f64>,
    pub network_received: Option<i64>,
    pub network_sent: Option<i64>,
    pub uptime: Option<i64>,
    pub host_type: Option<HostType>,
}

impl Host {
    /// Test if given `token` has expired and refresh it using the `refresh_token` if necessary
    pub fn verify_auth_token(token: HostAuthToken) -> Result<HostAuthToken> {
        if token.has_expired() {
            Err(ApiError::from(TokenError::Expired))
        } else {
            // Token is valid, just return what we got
            // If nothing was updated or changed, we don't even query for the user to save 1 query
            Ok(token)
        }
    }

    pub async fn toggle_online(
        host_id: Uuid,
        is_online: bool,
        conn: &mut AsyncPgConnection,
    ) -> Result<()> {
        let status = if is_online {
            ConnectionStatus::Online
        } else {
            ConnectionStatus::Offline
        };

        diesel::update(hosts::table.find(host_id))
            .set(hosts::status.eq(status))
            .execute(conn)
            .await?;

        Ok(())
    }

    pub async fn find_all(conn: &mut AsyncPgConnection) -> Result<Vec<Self>> {
        let hosts = hosts::table.get_results(conn).await?;
        Ok(hosts)
    }

    pub async fn find_by_node(node_id: Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let host = hosts::table
            .inner_join(nodes::table)
            .filter(nodes::id.eq(node_id))
            .select(hosts::all_columns)
            .get_result(conn)
            .await?;
        Ok(host)
    }

    pub async fn find_by_id(host_id: Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let host = hosts::table.find(host_id).get_result(conn).await?;
        Ok(host)
    }

    pub async fn find_by_name(name: &str, conn: &mut AsyncPgConnection) -> Result<Self> {
        let host = hosts::table
            .filter(hosts::name.eq(name))
            .get_result(conn)
            .await?;
        Ok(host)
    }

    pub async fn update_status(
        id: Uuid,
        host: HostStatusRequest,
        conn: &mut AsyncPgConnection,
    ) -> Result<Self> {
        let host = diesel::update(hosts::table.find(id))
            .set(host)
            .get_result(conn)
            .await?;
        Ok(host)
    }

    pub async fn delete(id: Uuid, conn: &mut AsyncPgConnection) -> Result<usize> {
        let n_rows = diesel::delete(hosts::table.find(id)).execute(conn).await?;
        Ok(n_rows)
    }

    /// We sum up all nodes values assigned to a host and deduct that from the total the host has
    /// We don't consider CPUs in the selection, hard disk is more important than memory. The result
    /// is ordered by disk_size and mem_size the first one in the list is returned
    pub async fn get_next_available_host_id(
        requirements: HardwareRequirements,
        conn: &mut AsyncPgConnection,
    ) -> Result<Uuid> {
        use diesel::sql_types::{BigInt, Uuid};

        #[derive(QueryableByName)]
        struct HostAndProps {
            #[diesel(sql_type = Uuid)]
            id: uuid::Uuid,
            #[diesel(sql_type = BigInt)]
            mem_size: i64,
            #[diesel(sql_type = BigInt)]
            disk_size: i64,
            #[diesel(sql_type = BigInt)]
            ip_addrs: i64,
        }

        let host: HostAndProps = diesel::sql_query(
            r#"
            SELECT hosts.id AS id,
                    COALESCE((hosts.mem_size - (SELECT SUM(mem_size_mb) FROM nodes WHERE host_id = hosts.id)::BIGINT), 0) AS mem_size,
                    COALESCE((hosts.disk_size - (SELECT SUM(disk_size_gb) FROM nodes WHERE host_id = hosts.id)::BIGINT), 0) AS disk_size,
                    (SELECT COUNT(*) FROM ip_addresses WHERE ip_addresses.host_id = hosts.id AND NOT ip_addresses.is_assigned)::BIGINT AS ip_addrs
            FROM hosts
            ORDER BY ip_addrs desc, disk_size desc, mem_size DESC
            LIMIT 1
        "#,
        )
        .get_result(conn)
        .await?;

        // Trace warnings, if the selected host doesn't seem to have enough resources
        if requirements.disk_size_gb > host.disk_size / 1024i64.pow(3) {
            tracing::warn!(
                "Host {} doesn't seem to have enough disk space available",
                host.id
            );
        }
        if requirements.mem_size_mb > host.mem_size / 1024i64.pow(2) {
            tracing::warn!(
                "Host {} doesn't seem to have enough memory available",
                host.id
            );
        }
        if host.ip_addrs < 1 {
            tracing::warn!(
                "Host {} doesn't seem to have enough IP addresses available",
                host.id
            );
        }

        Ok(host.id)
    }
}

#[axum::async_trait]
impl FindableById for Host {
    async fn find_by_id(id: Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let host = hosts::table.find(id).get_result(conn).await?;
        Ok(host)
    }
}

impl Identifiable for Host {
    fn get_id(&self) -> Uuid {
        self.id
    }
}

#[axum::async_trait]
impl Owned<Host, ()> for Host {
    async fn is_owned_by(&self, resource: Host, _db: ()) -> bool {
        self.id == resource.id
    }
}

#[tonic::async_trait]
impl super::UpdateInfo<blockjoy::HostInfo, Host> for Host {
    async fn update_info(info: blockjoy::HostInfo, conn: &mut AsyncPgConnection) -> Result<Host> {
        let update = info.as_update()?;
        update.update(conn).await
    }
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = hosts)]
pub struct NewHost<'a> {
    pub name: &'a str,
    pub version: Option<&'a str>,
    pub location: Option<&'a str>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<&'a str>,
    pub os_version: Option<&'a str>,
    pub ip_addr: &'a str,
    pub status: ConnectionStatus,
    pub ip_range_from: ipnetwork::IpNetwork,
    pub ip_range_to: ipnetwork::IpNetwork,
    pub ip_gateway: ipnetwork::IpNetwork,
}

impl NewHost<'_> {
    /// Creates a new `Host` in the db, including the necessary related rows.
    pub async fn create(self, conn: &mut AsyncPgConnection) -> Result<Host> {
        // Ensure gateway IP is not amongst the ones created in the IP range
        if super::IpAddress::in_range(
            self.ip_gateway.network(),
            self.ip_range_from.network(),
            self.ip_range_to.network(),
        ) {
            return Err(ApiError::IpGatewayError(anyhow!(
                "{} is in range {} - {}",
                self.ip_gateway,
                self.ip_range_from,
                self.ip_range_to
            )));
        }

        let host: Host = diesel::insert_into(hosts::table)
            .values(self)
            .get_result(conn)
            .await?;

        // Create IP range for new host
        let create_range = super::NewIpAddressRange::try_new(
            host.ip_range_from.ok_or_else(required("ip.range.from"))?,
            host.ip_range_to.ok_or_else(required("ip.range.to"))?,
            Some(host.id),
        )?;
        create_range.create(conn).await?;

        Ok(host)
    }
}

#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = hosts)]
pub struct UpdateHost<'a> {
    pub id: Uuid,
    pub name: Option<&'a str>,
    pub version: Option<&'a str>,
    pub location: Option<&'a str>,
    pub cpu_count: Option<i64>,
    pub mem_size: Option<i64>,
    pub disk_size: Option<i64>,
    pub os: Option<&'a str>,
    pub os_version: Option<&'a str>,
    pub ip_addr: Option<&'a str>,
    pub status: Option<ConnectionStatus>,
    pub ip_range_from: Option<ipnetwork::IpNetwork>,
    pub ip_range_to: Option<ipnetwork::IpNetwork>,
    pub ip_gateway: Option<ipnetwork::IpNetwork>,
}

impl UpdateHost<'_> {
    pub async fn update(self, conn: &mut AsyncPgConnection) -> Result<Host> {
        let host = diesel::update(hosts::table.find(self.id))
            .set(self)
            .get_result(conn)
            .await?;
        Ok(host)
    }
}

#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = hosts)]
pub struct UpdateHostMetrics {
    pub id: uuid::Uuid,
    pub used_cpu: Option<i32>,
    pub used_memory: Option<i64>,
    pub used_disk_space: Option<i64>,
    pub load_one: Option<f64>,
    pub load_five: Option<f64>,
    pub load_fifteen: Option<f64>,
    pub network_received: Option<i64>,
    pub network_sent: Option<i64>,
    pub uptime: Option<i64>,
}

impl UpdateHostMetrics {
    /// Performs a selective update of only the columns related to metrics of the provided nodes.
    pub async fn update_metrics(updates: Vec<Self>, conn: &mut AsyncPgConnection) -> Result<()> {
        for update in updates {
            diesel::update(hosts::table.find(update.id))
                .set(update)
                .execute(conn)
                .await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = hosts)]
pub struct HostStatusRequest {
    pub version: Option<String>,
    pub status: ConnectionStatus,
}
