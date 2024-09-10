use std::collections::HashSet;

use derive_more::{Deref, Display, From, FromStr};
use diesel::prelude::PgArrayExpressionMethods;
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;

use crate::auth::resource::OrgId;
use crate::database::Conn;
use crate::grpc::{api, Status};
use crate::model::schema::archives;
use crate::store::StoreId;

use super::{ImageId, ImagePropertyId};

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to bulk create new archives: {0}
    BulkCreate(diesel::result::Error),
    /// Failed to find archive for id `{0:?}`: {1}
    ById(ArchiveId, diesel::result::Error),
    /// Failed to find archives for image id `{0:?}`: {1}
    ByImageId(ImageId, diesel::result::Error),
    /// Failed to find archives for image id `{0:?}` and property ids `{1:?}`: {2}
    ByPropertyIds(ImageId, Vec<ImagePropertyId>, diesel::result::Error),
    /// Failed to update archive {0}: {1}
    Update(ArchiveId, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ById(_, NotFound) | Update(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct ArchiveId(uuid::Uuid);

#[derive(Clone, Debug, Queryable)]
#[diesel(table_name = archives)]
pub struct Archive {
    pub id: ArchiveId,
    pub org_id: Option<OrgId>,
    pub image_id: ImageId,
    pub store_id: StoreId,
    pub image_property_ids: Vec<Option<ImagePropertyId>>,
}

impl Archive {
    pub async fn by_id(
        id: ArchiveId,
        org_id: Option<OrgId>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        archives::table
            .find(id)
            .filter(archives::org_id.eq(org_id).or(archives::org_id.is_null()))
            .get_result(conn)
            .await
            .map_err(|err| Error::ById(id, err))
    }

    pub async fn by_image_id(
        image_id: ImageId,
        org_id: Option<OrgId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        archives::table
            .filter(archives::image_id.eq(image_id))
            .filter(archives::org_id.eq(org_id).or(archives::org_id.is_null()))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByImageId(image_id, err))
    }

    pub async fn by_property_ids(
        image_id: ImageId,
        org_id: Option<OrgId>,
        property_ids: Vec<ImagePropertyId>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        archives::table
            .filter(archives::image_id.eq(image_id))
            .filter(archives::org_id.eq(org_id).or(archives::org_id.is_null()))
            .filter(archives::image_property_ids.contains(&property_ids))
            .filter(archives::image_property_ids.is_contained_by(&property_ids))
            .get_result(conn)
            .await
            .map_err(|err| Error::ByPropertyIds(image_id, property_ids, err))
    }
}

impl From<Archive> for api::Archive {
    fn from(archive: Archive) -> Self {
        api::Archive {
            archive_id: archive.id.to_string(),
            image_id: archive.image_id.to_string(),
            store_id: archive.store_id.into(),
            image_property_ids: archive
                .image_property_ids
                .iter()
                .filter_map(|id| id.map(|id| id.to_string()))
                .collect(),
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = archives)]
pub struct NewArchive {
    pub image_id: ImageId,
    pub store_id: StoreId,
    pub image_property_ids: Vec<Option<ImagePropertyId>>,
}

impl NewArchive {
    pub fn new(
        image_id: ImageId,
        store_id: StoreId,
        property_ids: &HashSet<ImagePropertyId>,
    ) -> Self {
        NewArchive {
            image_id,
            store_id,
            image_property_ids: property_ids.iter().map(|id| Some(*id)).collect(),
        }
    }

    pub async fn bulk_create(
        archives: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Archive>, Error> {
        diesel::insert_into(archives::table)
            .values(archives)
            .get_results(conn)
            .await
            .map_err(Error::BulkCreate)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = archives)]
pub struct UpdateArchive {
    pub id: ArchiveId,
    pub store_id: Option<StoreId>,
}

impl UpdateArchive {
    pub async fn update(self, conn: &mut Conn<'_>) -> Result<Archive, Error> {
        let id = self.id;
        diesel::update(archives::table.find(id))
            .set(self)
            .get_result(conn)
            .await
            .map_err(|err| Error::Update(id, err))
    }
}
