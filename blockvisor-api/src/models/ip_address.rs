use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use diesel::dsl;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use ipnet::{IpAddrRange, Ipv4AddrRange};
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

pub struct NewIpAddressRange {
    from: IpAddr,
    to: IpAddr,
    host_id: HostId,
}

impl NewIpAddressRange {
    pub fn try_new(from: IpAddr, to: IpAddr, host_id: HostId) -> Result<Self, Error> {
        if to < from {
            return Err(Error::ToIpBeforeFrom);
        }

        Ok(NewIpAddressRange { from, to, host_id })
    }

    pub async fn create(
        self,
        exclude: &[IpAddr],
        conn: &mut Conn<'_>,
    ) -> Result<Vec<IpAddress>, Error> {
        let host_id = self.host_id;
        let start_range = Self::to_ipv4(self.from)?;
        let stop_range = Self::to_ipv4(self.to)?;
        let ip_addrs = IpAddrRange::from(Ipv4AddrRange::new(start_range, stop_range));
        let ip_addrs: Vec<_> = ip_addrs
            .into_iter()
            .filter(|ip| !exclude.contains(ip))
            .map(|ip| CreateIpAddress {
                ip: ip.into(),
                host_id,
            })
            .collect();

        diesel::insert_into(ip_addresses::table)
            .values(ip_addrs)
            .get_results(conn)
            .await
            .map_err(Error::Create)
    }

    const fn to_ipv4(addr: IpAddr) -> Result<Ipv4Addr, Error> {
        match addr {
            IpAddr::V4(v4) => Ok(v4),
            IpAddr::V6(v6) => Err(Error::UnexpectedIpv6(v6)),
        }
    }
}

#[derive(Debug, Queryable)]
pub struct IpAddress {
    pub(crate) id: uuid::Uuid,
    pub(crate) ip: IpNetwork,
    #[allow(unused)]
    pub(crate) host_id: Option<HostId>,
    #[allow(unused)]
    pub(crate) is_assigned: bool,
}

impl IpAddress {
    /// Helper returning the next valid IP address for host identified by `host_id`
    pub async fn next_for_host(host_id: HostId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let ip: Self = ip_addresses::table
            .filter(ip_addresses::host_id.eq(host_id))
            .filter(ip_addresses::is_assigned.eq(false))
            .get_result(conn)
            .await
            .map_err(Error::NextForHost)?;

        Self::assign(ip.id, host_id, conn).await
    }

    /// Helper assigned IP address identified by `ìd` to host identified by `host_id`
    pub async fn assign(
        id: uuid::Uuid,
        host_id: HostId,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let fields = UpdateIpAddress {
            id,
            host_id: Some(host_id),
            is_assigned: Some(true),
        };

        fields.update(conn).await
    }

    /// Helper assigned IP address identified by `ìd` to host identified by `host_id`
    pub async fn unassign(
        id: uuid::Uuid,
        host_id: HostId,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let fields = UpdateIpAddress {
            id,
            host_id: Some(host_id),
            is_assigned: Some(false),
        };

        fields.update(conn).await
    }

    pub fn in_range(ip: IpAddr, from: IpAddr, to: IpAddr) -> bool {
        from < ip && to > ip
    }

    pub fn ip(&self) -> String {
        self.ip.ip().to_string()
    }

    pub async fn assigned(ip: IpAddr, conn: &mut Conn<'_>) -> Result<bool, Error> {
        let ip = IpNetwork::new(ip, 32).map_err(Error::NewIpNetwork)?;
        let row = ip_addresses::table.filter(ip_addresses::ip.eq(ip));

        diesel::select(dsl::exists(row))
            .get_result(conn)
            .await
            .map_err(Error::Assigned)
    }

    pub async fn find_by_ip(ip: IpAddr, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let ip_network = IpNetwork::new(ip, 32).map_err(Error::NewIpNetwork)?;
        ip_addresses::table
            .filter(ip_addresses::ip.eq(ip_network))
            .get_result(conn)
            .await
            .map_err(|err| Error::FindByIp(ip, err))
    }

    pub async fn find_by_hosts(
        host_ids: HashSet<HostId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        ip_addresses::table
            .filter(ip_addresses::host_id.eq_any(&host_ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByHosts(host_ids, err))
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = ip_addresses)]
pub struct UpdateIpAddress {
    pub(crate) id: uuid::Uuid,
    pub(crate) host_id: Option<HostId>,
    pub(crate) is_assigned: Option<bool>,
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
    use crate::config::Context;

    use super::*;

    #[tokio::test]
    async fn should_create_ip_range() {
        let (_ctx, db) = Context::with_mocked().await.unwrap();
        let mut conn = db.conn().await;

        let from = "192.129.0.10".parse().unwrap();
        let to = "192.129.0.20".parse().unwrap();
        let new_range = NewIpAddressRange::try_new(from, to, db.seed.host.id).unwrap();
        let range = new_range.create(&[], &mut conn).await.unwrap();

        assert_eq!(range.len(), 11);
    }

    #[tokio::test]
    #[should_panic(expected = "called `Result::unwrap()` on an `Err` value: ToIpBeforeFrom")]
    async fn should_fail_creating_ip_range() {
        let (_ctx, db) = Context::with_mocked().await.unwrap();

        let from = "192.129.0.20".parse().unwrap();
        let to = "192.129.0.10".parse().unwrap();
        NewIpAddressRange::try_new(from, to, db.seed.host.id).unwrap();
    }

    #[test]
    fn should_fail_if_ip_in_range() {
        let ref_ip = "192.168.0.15".parse().unwrap();
        let from_ip = "192.168.0.10".parse().unwrap();
        let to_ip = "192.168.0.10".parse().unwrap();

        assert!(!IpAddress::in_range(ref_ip, from_ip, to_ip));
    }
}
