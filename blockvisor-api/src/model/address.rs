use derive_more::{Deref, Display, From, FromStr};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::OrgId;
use crate::database::Conn;
use crate::grpc::Status;

use super::schema::addresses;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to create new address: {0}
    Create(diesel::result::Error),
    /// Failed to delete address: {0}
    Delete(diesel::result::Error),
    /// Failed to find address id `{0}`: {1}
    FindById(AddressId, diesel::result::Error),
    /// Failed to find address by org id `{0}`: {1}
    FindByOrgId(OrgId, diesel::result::Error),
    /// Failed update address: {0}
    Update(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            FindById(_, NotFound) | FindByOrgId(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct AddressId(Uuid);

#[derive(Clone, Debug, Queryable, AsChangeset)]
#[diesel(table_name = addresses)]
pub struct Address {
    pub id: AddressId,
    pub city: Option<String>,
    pub country: Option<String>,
    pub line1: Option<String>,
    pub line2: Option<String>,
    pub postal_code: Option<String>,
    pub state: Option<String>,
}

impl Address {
    pub async fn by_id(id: AddressId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        addresses::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn by_org_id(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        use super::schema::orgs;
        orgs::table
            .inner_join(addresses::table)
            .filter(orgs::id.eq(org_id))
            .select(addresses::all_columns)
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByOrgId(org_id, err))
    }

    pub async fn update(self, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(addresses::table.filter(addresses::id.eq(self.id)))
            .set(&self)
            .get_result(conn)
            .await
            .map_err(Error::Update)
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = addresses)]
pub struct NewAddress<'a> {
    city: Option<&'a str>,
    country: Option<&'a str>,
    line1: Option<&'a str>,
    line2: Option<&'a str>,
    postal_code: Option<&'a str>,
    state: Option<&'a str>,
}

impl<'a> NewAddress<'a> {
    pub const fn new(
        city: Option<&'a str>,
        country: Option<&'a str>,
        line1: Option<&'a str>,
        line2: Option<&'a str>,
        postal_code: Option<&'a str>,
        state: Option<&'a str>,
    ) -> Self {
        Self {
            city,
            country,
            line1,
            line2,
            postal_code,
            state,
        }
    }

    pub async fn create(self, conn: &mut Conn<'_>) -> Result<Address, Error> {
        diesel::insert_into(addresses::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}
