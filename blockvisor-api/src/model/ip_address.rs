use std::collections::HashSet;
use std::net::{IpAddr, Ipv6Addr};

use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::HostId;
use crate::database::Conn;
use crate::grpc::Status;
use crate::util::sql::IpNetwork;

use super::schema::{ip_addresses, nodes};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to assign ip address: {0}
    Assign(diesel::result::Error),
    /// Failed to find assigned ip address range: {0}
    Assigned(diesel::result::Error),
    /// Failed to create new ip address range: {0}
    Create(diesel::result::Error),
    /// Failed to delete ip addresses for host {0}: {1}
    DeleteByHostId(HostId, diesel::result::Error),
    /// Failed to find ip address for hosts `{0:?}`: {1}
    FindByHosts(HashSet<HostId>, diesel::result::Error),
    /// Failed to find ip address for ip `{0}`: {1}
    FindByIp(IpAddr, diesel::result::Error),
    /// Failed to find ip addresses in use: {0}
    InUse(diesel::result::Error),
    /// Failed to lock table `nodes`: {0}
    Lock(diesel::result::Error),
    /// Failed to get next IP for host: {0}
    NextForHost(diesel::result::Error),
    /// To IP address is before the From IP.
    ToIpBeforeFrom,
    /// Failed to unassign ip address: {0}
    Unassign(diesel::result::Error),
    /// Unexpected IPv6 in the database: {0}
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
            NextForHost(NotFound) => {
                Status::failed_precondition("This host has no available ip addresses")
            }
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
    pub id: Uuid,
    pub ip: IpNetwork,
    pub host_id: HostId,
}

impl IpAddress {
    pub async fn next_for_host(
        host_id: HostId,
        conn: &mut Conn<'_>,
    ) -> Result<Option<Self>, Error> {
        let ids_in_use: Vec<Uuid> = ip_addresses::table
            .left_join(nodes::table.on(ip_addresses::ip.eq(nodes::ip_address)))
            .filter(ip_addresses::host_id.eq(host_id))
            .filter(nodes::id.is_not_null())
            .filter(nodes::deleted_at.is_null())
            .select(ip_addresses::id)
            .load(conn)
            .await
            .map_err(Error::InUse)?;

        let result = ip_addresses::table
            .filter(ip_addresses::host_id.eq(host_id))
            .filter(ip_addresses::id.ne_all(ids_in_use))
            .select(ip_addresses::all_columns)
            .limit(1)
            .for_update()
            .skip_locked()
            .get_result(conn)
            .await;

        match result {
            Ok(ip) => Ok(Some(ip)),
            Err(diesel::result::Error::NotFound) => Ok(None),
            Err(err) => Err(Error::NextForHost(err)),
        }
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

    pub async fn delete_by_host_id(host_id: HostId, conn: &mut Conn<'_>) -> Result<(), Error> {
        diesel::delete(ip_addresses::table.filter(ip_addresses::host_id.eq(host_id)))
            .execute(conn)
            .await
            .map_err(|err| Error::DeleteByHostId(host_id, err))?;
        Ok(())
    }
}
