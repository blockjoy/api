use std::collections::HashSet;
use std::net::{IpAddr, Ipv6Addr};

use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use ipnetwork::IpNetwork;
use thiserror::Error;
use tonic::Status;

use crate::auth::resource::HostId;
use crate::database::Conn;

use super::schema::ip_addresses;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to find assigned ip address range: {0}
    Assigned(diesel::result::Error),
    /// Failed to create new ip address range: {0}
    Create(diesel::result::Error),
    /// Failed to find ip address for hosts `{0:?}`: {1}
    FindByHosts(HashSet<HostId>, diesel::result::Error),
    /// Failed to find ip address for ip `{0}`: {1}
    FindByIp(IpAddr, diesel::result::Error),
    /// Failed to get next IP for host: {0}
    NextForHost(diesel::result::Error),
    /// Failed to create new IP network: {0}
    NewIpNetwork(ipnetwork::IpNetworkError),
    /// To IP address is before the From IP.
    ToIpBeforeFrom,
    /// Unexpected IP v6 in the database: {0}
    UnexpectedIpv6(Ipv6Addr),
    /// Failed to update ip address range: {0}
    Update(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            FindByIp(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = ip_addresses)]
pub struct CreateIpAddress {
    pub ip: IpNetwork,
    pub host_id: HostId,
}

impl CreateIpAddress {
    pub const fn new(ip: IpNetwork, host_id: HostId) -> Self {
        Self { ip, host_id }
    }

    pub async fn bulk_create(ips: Vec<Self>, conn: &mut Conn<'_>) -> Result<Vec<IpAddress>, Error> {
        diesel::insert_into(ip_addresses::table)
            .values(ips)
            .get_results(conn)
            .await
            .map_err(Error::Create)
    }
}

#[derive(Debug, Queryable)]
pub struct IpAddress {
    pub id: uuid::Uuid,
    pub ip: IpNetwork,
    pub host_id: Option<HostId>,
}

impl IpAddress {
    /// Helper returning the next valid IP address for host identified by `host_id`
    pub async fn by_host_unassigned(host_id: HostId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        use super::schema::nodes;

        let node_for_this_ip = nodes::ip
            .eq(ip_addresses::ip)
            .and(nodes::deleted_at.is_null());
        ip_addresses::table
            .filter(ip_addresses::host_id.eq(host_id))
            .left_join(nodes::table.on(node_for_this_ip))
            .filter(nodes::id.is_null())
            .select(ip_addresses::all_columns)
            .get_result(conn)
            .await
            .map_err(Error::NextForHost)
    }

    pub fn in_range(ip: IpAddr, from: IpAddr, to: IpAddr) -> bool {
        from < ip && to > ip
    }

    pub fn ip(&self) -> IpAddr {
        self.ip.ip()
    }

    pub async fn by_ip(ip: IpAddr, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let ip_network = IpNetwork::new(ip, 32).map_err(Error::NewIpNetwork)?;
        ip_addresses::table
            .filter(ip_addresses::ip.eq(ip_network))
            .get_result(conn)
            .await
            .map_err(|err| Error::FindByIp(ip, err))
    }

    pub async fn by_host_ids(
        host_ids: &HashSet<HostId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        ip_addresses::table
            .filter(ip_addresses::host_id.eq_any(host_ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByHosts(host_ids.clone(), err))
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = ip_addresses)]
pub struct UpdateIpAddress {
    pub id: uuid::Uuid,
    pub host_id: Option<HostId>,
}

impl UpdateIpAddress {
    pub async fn update(self, conn: &mut Conn<'_>) -> Result<IpAddress, Error> {
        diesel::update(ip_addresses::table.find(self.id))
            .set(self)
            .get_result(conn)
            .await
            .map_err(Error::Update)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn should_fail_if_ip_in_range() {
        let ref_ip = "192.168.0.15".parse().unwrap();
        let from_ip = "192.168.0.10".parse().unwrap();
        let to_ip = "192.168.0.10".parse().unwrap();

        assert!(!IpAddress::in_range(ref_ip, from_ip, to_ip));
    }
}
