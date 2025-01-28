use std::collections::HashSet;

use derive_more::{Deref, From};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::HostId;
use crate::database::Conn;
use crate::grpc::Status;
use crate::model::sql::IpNetwork;

use super::schema::{ip_addresses, nodes};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to find assigned ip addresses for hosts `{0:?}`: {1}
    AssignedForHosts(HashSet<HostId>, diesel::result::Error),
    /// Failed to create new ip addresses: {0}
    BulkCreate(diesel::result::Error),
    /// Failed to delete ip addresses for host {0}: {1}
    DeleteForHost(HostId, diesel::result::Error),
    /// Failed to find ip address for hosts `{0:?}`: {1}
    FindForHosts(HashSet<HostId>, diesel::result::Error),
    /// Failed to find ip addresses in use: {0}
    FindInUse(diesel::result::Error),
    /// Failed to get next IP for host {0}: {1}
    NextForHost(HostId, diesel::result::Error),
    /// Failed to update ip address range: {0}
    Update(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            BulkCreate(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("Ip address already exists.")
            }
            AssignedForHosts(_, NotFound) => Status::not_found("Not found."),
            NextForHost(_, NotFound) => Status::failed_precondition("host has no ips"),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From)]
pub struct IpAddressId(Uuid);

#[derive(Debug, Queryable)]
pub struct IpAddress {
    pub id: IpAddressId,
    pub ip: IpNetwork,
    pub host_id: HostId,
}

impl IpAddress {
    pub async fn for_hosts(
        host_ids: &HashSet<HostId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        ip_addresses::table
            .filter(ip_addresses::host_id.eq_any(host_ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindForHosts(host_ids.clone(), err))
    }

    pub async fn assigned_for_hosts(
        host_ids: &HashSet<HostId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        ip_addresses::table
            .left_join(nodes::table.on(ip_addresses::ip.eq(nodes::ip_address)))
            .filter(ip_addresses::host_id.eq_any(host_ids))
            .filter(nodes::id.is_not_null())
            .filter(nodes::deleted_at.is_null())
            .select(ip_addresses::all_columns)
            .get_results(conn)
            .await
            .map_err(|err| Error::AssignedForHosts(host_ids.clone(), err))
    }

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
            .map_err(Error::FindInUse)?;

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
            Err(NotFound) => Ok(None),
            Err(err) => Err(Error::NextForHost(host_id, err)),
        }
    }

    pub async fn delete_for_host(host_id: HostId, conn: &mut Conn<'_>) -> Result<(), Error> {
        diesel::delete(ip_addresses::table.filter(ip_addresses::host_id.eq(host_id)))
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(|err| Error::DeleteForHost(host_id, err))
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = ip_addresses)]
pub struct NewIpAddress {
    pub ip: IpNetwork,
    pub host_id: HostId,
}

impl NewIpAddress {
    pub const fn new(ip: IpNetwork, host_id: HostId) -> Self {
        Self { ip, host_id }
    }

    pub async fn bulk_create(ips: Vec<Self>, conn: &mut Conn<'_>) -> Result<Vec<IpAddress>, Error> {
        diesel::insert_into(ip_addresses::table)
            .values(ips)
            .get_results(conn)
            .await
            .map_err(Error::BulkCreate)
    }
}
