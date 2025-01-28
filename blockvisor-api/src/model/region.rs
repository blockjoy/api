use std::collections::HashSet;

use derive_more::{Deref, Display, From, FromStr, Into};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use uuid::Uuid;

use crate::database::Conn;
use crate::grpc::{api, Status};
use crate::util::LOWER_KEBAB_CASE;

use super::schema::regions;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to create region: {0}
    Create(diesel::result::Error),
    /// Failed to get regions for id `{0}`: {1}
    ById(RegionId, diesel::result::Error),
    /// Failed to get regions for ids `{0:?}`: {1}
    ByIds(HashSet<RegionId>, diesel::result::Error),
    /// Failed to get regions for key `{0}`: {1}
    ByKey(RegionKey, diesel::result::Error),
    /// Failed to parse free_ips as u32: {0}
    FreeIps(std::num::TryFromIntError),
    /// Region key is not lower-kebab-case: {0}
    KeyChars(String),
    /// Region key must be at least 3 characters: {0}
    KeyLen(String),
    /// Failed to update region id `{0}`: {1}
    Update(RegionId, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("Region already exists.")
            }
            ById(_, NotFound) | ByIds(_, NotFound) => Status::not_found("Not found."),
            KeyChars(_) | KeyLen(_) => Status::invalid_argument("region_key"),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct RegionId(Uuid);

#[derive(Clone, Debug, Display, PartialEq, Eq, DieselNewType, Deref, Into)]
pub struct RegionKey(String);

impl RegionKey {
    pub fn new(key: String) -> Result<Self, Error> {
        if key.len() < 3 {
            Err(Error::KeyLen(key))
        } else if !key.chars().all(|c| LOWER_KEBAB_CASE.contains(c)) {
            Err(Error::KeyChars(key))
        } else {
            Ok(RegionKey(key))
        }
    }
}

#[derive(Clone, Debug, Queryable)]
pub struct Region {
    pub id: RegionId,
    pub sku_code: Option<String>,
    pub key: RegionKey,
    pub display_name: String,
}

impl Region {
    pub async fn by_id(id: RegionId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        regions::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::ById(id, err))
    }

    pub async fn by_ids(
        region_ids: &HashSet<RegionId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        regions::table
            .filter(regions::id.eq_any(region_ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByIds(region_ids.clone(), err))
    }

    pub async fn by_key(key: &RegionKey, conn: &mut Conn<'_>) -> Result<Self, Error> {
        regions::table
            .filter(regions::key.eq(key))
            .get_result(conn)
            .await
            .map_err(|err| Error::ByKey(key.clone(), err))
    }
}

#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = regions)]
pub struct NewRegion<'a> {
    pub key: RegionKey,
    pub display_name: &'a str,
    pub sku_code: Option<&'a str>,
}

impl NewRegion<'_> {
    pub async fn create(self, conn: &mut Conn<'_>) -> Result<Region, Error> {
        diesel::insert_into(regions::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}

impl From<Region> for api::Region {
    fn from(region: Region) -> Self {
        api::Region {
            region_id: region.id.to_string(),
            region_key: region.key.into(),
            display_name: region.display_name,
            sku_code: region.sku_code,
        }
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = regions)]
pub struct UpdateRegion<'u> {
    pub id: RegionId,
    pub display_name: Option<&'u str>,
    pub sku_code: Option<&'u str>,
}

impl UpdateRegion<'_> {
    pub async fn apply(self, conn: &mut Conn<'_>) -> Result<Region, Error> {
        let id = self.id;
        diesel::update(regions::table.find(id))
            .set(self)
            .get_result(conn)
            .await
            .map_err(|err| Error::Update(id, err))
    }
}
