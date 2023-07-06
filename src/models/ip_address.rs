use super::schema::ip_addresses;
use crate::{Error, Result};
use anyhow::anyhow;
use diesel::{dsl, prelude::*};
use diesel_async::RunQueryDsl;
use ipnet::{IpAddrRange, Ipv4AddrRange};
use std::net::IpAddr;

#[derive(Debug, Insertable)]
#[diesel(table_name = ip_addresses)]
pub struct CreateIpAddress {
    pub ip: ipnetwork::IpNetwork,
    pub host_id: uuid::Uuid,
}

pub struct NewIpAddressRange {
    from: IpAddr,
    to: IpAddr,
    host_id: uuid::Uuid,
}

impl NewIpAddressRange {
    pub fn try_new(from: IpAddr, to: IpAddr, host_id: uuid::Uuid) -> Result<Self> {
        if to < from {
            Err(Error::UnexpectedError(anyhow!(
                "TO IP can't be smaller as FROM IP"
            )))
        } else {
            Ok(Self { from, to, host_id })
        }
    }

    pub async fn create(
        self,
        exclude: &[IpAddr],
        conn: &mut super::Conn,
    ) -> Result<Vec<IpAddress>> {
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

        let ip_addrs = diesel::insert_into(ip_addresses::table)
            .values(ip_addrs)
            .get_results(conn)
            .await?;
        Ok(ip_addrs)
    }

    fn to_ipv4(addr: IpAddr) -> Result<std::net::Ipv4Addr> {
        match addr {
            IpAddr::V4(v4) => Ok(v4),
            IpAddr::V6(v6) => Err(anyhow!("Found v6 ip addr in database: {v6}").into()),
        }
    }
}

#[derive(Debug, Queryable)]
pub struct IpAddress {
    pub(crate) id: uuid::Uuid,
    pub(crate) ip: ipnetwork::IpNetwork,
    #[allow(unused)]
    pub(crate) host_id: Option<uuid::Uuid>,
    #[allow(unused)]
    pub(crate) is_assigned: bool,
}

impl IpAddress {
    /// Helper returning the next valid IP address for host identified by `host_id`
    pub async fn next_for_host(host_id: uuid::Uuid, conn: &mut super::Conn) -> Result<Self> {
        let ip: Self = ip_addresses::table
            .filter(ip_addresses::host_id.eq(host_id))
            .filter(ip_addresses::is_assigned.eq(false))
            .get_result(conn)
            .await
            .map_err(|_| crate::Error::unexpected("No more ip's available"))?;

        Self::assign(ip.id, host_id, conn).await
    }

    /// Helper assigned IP address identified by `ìd` to host identified by `host_id`
    pub async fn assign(
        id: uuid::Uuid,
        host_id: uuid::Uuid,
        conn: &mut super::Conn,
    ) -> Result<Self> {
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
        host_id: uuid::Uuid,
        conn: &mut super::Conn,
    ) -> Result<Self> {
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

    pub async fn assigned(ip: IpAddr, conn: &mut super::Conn) -> Result<bool> {
        let ip = ipnetwork::IpNetwork::new(ip, 32)?;
        let row = ip_addresses::table.filter(ip_addresses::ip.eq(ip));
        let assigned = diesel::select(dsl::exists(row)).get_result(conn).await?;
        Ok(assigned)
    }

    pub async fn find_by_node(node_ip: IpAddr, conn: &mut super::Conn) -> Result<Self> {
        let ip = ipnetwork::IpNetwork::new(node_ip, 32)?;
        let ip = ip_addresses::table
            .filter(ip_addresses::ip.eq(ip))
            .get_result(conn)
            .await?;
        Ok(ip)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = ip_addresses)]
pub struct UpdateIpAddress {
    pub(crate) id: uuid::Uuid,
    pub(crate) host_id: Option<uuid::Uuid>,
    pub(crate) is_assigned: Option<bool>,
}

impl UpdateIpAddress {
    pub async fn update(self, conn: &mut super::Conn) -> Result<IpAddress> {
        let ip = diesel::update(ip_addresses::table.find(self.id))
            .set(self)
            .get_result(conn)
            .await?;
        Ok(ip)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::Context;
    use crate::TestDb;

    #[tokio::test]
    async fn should_create_ip_range() -> anyhow::Result<()> {
        let context = Context::new_with_default_toml().unwrap();
        let db = TestDb::setup(context).await;

        let new_range = NewIpAddressRange::try_new(
            "192.129.0.10".parse().unwrap(),
            "192.129.0.20".parse().unwrap(),
            db.host().await.id,
        )?;
        let mut conn = db.conn().await;
        let range = new_range.create(&[], &mut conn).await?;
        assert_eq!(range.len(), 11);

        Ok(())
    }

    #[tokio::test]
    #[should_panic]
    async fn should_fail_creating_ip_range() {
        let context = Context::new_with_default_toml().unwrap();
        let db = TestDb::setup(context).await;

        NewIpAddressRange::try_new(
            "192.129.0.20".parse().unwrap(),
            "192.129.0.10".parse().unwrap(),
            db.host().await.id,
        )
        .unwrap();
    }

    #[test]
    fn should_fail_if_ip_in_range() {
        let ref_ip = "192.168.0.15".parse().unwrap();
        let from_ip = "192.168.0.10".parse().unwrap();
        let to_ip = "192.168.0.10".parse().unwrap();

        assert!(!IpAddress::in_range(ref_ip, from_ip, to_ip));
    }
}
