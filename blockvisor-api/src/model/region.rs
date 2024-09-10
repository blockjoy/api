use std::collections::HashSet;

use derive_more::{Deref, Display, From};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use uuid::Uuid;

use crate::database::Conn;
use crate::grpc::Status;

use super::schema::regions;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to get regions for id `{0}`: {1}
    ById(RegionId, diesel::result::Error),
    /// Failed to get regions by ids `{0:?}`: {1}
    ByIds(HashSet<RegionId>, diesel::result::Error),
    /// Failed to get regions for name `{0}`: {1}
    ByName(String, diesel::result::Error),
    /// Failed to get or create regions for name `{0}`: {1}
    GetOrCreate(String, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ById(_, NotFound) | ByIds(_, NotFound) | ByName(_, NotFound) => {
                Status::not_found("Not found.")
            }
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From)]
pub struct RegionId(Uuid);

#[derive(Clone, Debug, Queryable)]
pub struct Region {
    pub id: RegionId,
    pub name: String,
    pub pricing_tier: Option<String>,
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

    pub async fn by_name(name: &str, conn: &mut Conn<'_>) -> Result<Self, Error> {
        regions::table
            .filter(regions::name.eq(name))
            .get_result(conn)
            .await
            .map_err(|err| Error::ByName(name.into(), err))
    }

    pub async fn get_or_create(
        name: &str,
        pricing_tier: Option<&str>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        diesel::insert_into(regions::table)
            .values((
                regions::name.eq(name.to_lowercase()),
                regions::pricing_tier.eq(pricing_tier.map(str::to_uppercase)),
            ))
            .on_conflict(regions::name)
            .do_update()
            .set(regions::name.eq(name.to_lowercase()))
            .get_result(conn)
            .await
            .map_err(|err| Error::GetOrCreate(name.into(), err))
    }
}
