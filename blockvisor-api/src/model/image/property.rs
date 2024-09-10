use std::collections::HashSet;

use derive_more::{Deref, Display, From};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use uuid::Uuid;

use crate::database::Conn;
use crate::grpc::{api, common, Status};
use crate::model::schema::{image_properties, sql_types};

use super::ImageId;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to bulk create image properties: {0}
    BulkCreate(diesel::result::Error),
    /// Failed to find image property for id `{0}`: {1}
    ById(ImagePropertyId, diesel::result::Error),
    /// Failed to find image properties for property ids `{0:?}`: {1}
    ByIds(HashSet<ImagePropertyId>, diesel::result::Error),
    /// Failed to find image properties for image id `{0:?}`: {1}
    ByImageId(ImageId, diesel::result::Error),
    /// Failed to find image properties for image ids `{0:?}`: {1}
    ByImageIds(HashSet<ImageId>, diesel::result::Error),
    /// Missing UiType.
    MissingUiType,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ById(_, NotFound) => Status::not_found("Not found."),
            MissingUiType => Status::invalid_argument("property.ui_type"),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From)]
pub struct ImagePropertyId(Uuid);

#[derive(Clone, derive_more::Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From)]
#[debug("{_0}")]
pub struct ImagePropertyKey(pub String);

#[derive(Clone, Debug, Queryable)]
#[diesel(table_name = image_properties)]
pub struct ImageProperty {
    pub id: ImagePropertyId,
    pub image_id: ImageId,
    pub key: ImagePropertyKey,
    pub key_group: Option<String>,
    pub is_group_default: Option<bool>,
    pub new_archive: bool,
    pub default_value: String,
    pub dynamic_value: bool,
    pub description: Option<String>,
    pub ui_type: UiType,
    pub add_cpu_cores: Option<i64>,
    pub add_memory_bytes: Option<i64>,
    pub add_disk_bytes: Option<i64>,
}

impl ImageProperty {
    pub async fn by_id(id: ImagePropertyId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        image_properties::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::ById(id, err))
    }

    pub async fn by_ids(
        ids: &HashSet<ImagePropertyId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        image_properties::table
            .filter(image_properties::id.eq_any(ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByIds(ids.clone(), err))
    }

    pub async fn by_image_id(image_id: ImageId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        image_properties::table
            .filter(image_properties::image_id.eq(image_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByImageId(image_id, err))
    }

    pub async fn by_image_ids(
        image_ids: &HashSet<ImageId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        image_properties::table
            .filter(image_properties::image_id.eq_any(image_ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByImageIds(image_ids.clone(), err))
    }
}

impl From<ImageProperty> for api::ImageProperty {
    fn from(property: ImageProperty) -> Self {
        api::ImageProperty {
            image_property_id: property.id.to_string(),
            image_id: property.image_id.to_string(),
            key: property.key.0,
            key_group: property.key_group,
            is_group_default: property.is_group_default,
            new_archive: property.new_archive,
            default_value: property.default_value,
            dynamic_value: property.dynamic_value,
            description: property.description,
            ui_type: common::UiType::from(property.ui_type).into(),
            add_cpu_cores: property.add_cpu_cores,
            add_memory_bytes: property.add_memory_bytes,
            add_disk_bytes: property.add_disk_bytes,
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = image_properties)]
pub struct NewProperty {
    pub image_id: ImageId,
    pub key: ImagePropertyKey,
    pub key_group: Option<String>,
    pub is_group_default: Option<bool>,
    pub new_archive: bool,
    pub default_value: String,
    pub dynamic_value: bool,
    pub description: Option<String>,
    pub ui_type: UiType,
    pub add_cpu_cores: Option<i64>,
    pub add_memory_bytes: Option<i64>,
    pub add_disk_bytes: Option<i64>,
}

impl NewProperty {
    pub fn new(image_id: ImageId, property: ImageProperty) -> Self {
        NewProperty {
            image_id,
            key: property.key,
            key_group: property.key_group,
            is_group_default: property.is_group_default,
            new_archive: property.new_archive,
            default_value: property.default_value,
            dynamic_value: property.dynamic_value,
            description: property.description,
            ui_type: property.ui_type,
            add_cpu_cores: property.add_cpu_cores,
            add_memory_bytes: property.add_memory_bytes,
            add_disk_bytes: property.add_disk_bytes,
        }
    }

    pub fn from(image_id: ImageId, property: api::AddImageProperty) -> Result<Self, Error> {
        let ui_type = Option::from(property.ui_type()).ok_or(Error::MissingUiType)?;

        Ok(NewProperty {
            image_id,
            key: ImagePropertyKey(property.key),
            key_group: property.key_group,
            is_group_default: property.is_group_default,
            new_archive: property.new_archive,
            default_value: property.default_value,
            dynamic_value: property.dynamic_value,
            description: property.description,
            ui_type,
            add_cpu_cores: property.add_cpu_cores,
            add_memory_bytes: property.add_memory_bytes,
            add_disk_bytes: property.add_disk_bytes,
        })
    }

    pub async fn bulk_create(
        properties: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<ImageProperty>, Error> {
        diesel::insert_into(image_properties::table)
            .values(properties)
            .get_results(conn)
            .await
            .map_err(Error::BulkCreate)
    }
}

#[derive(Clone, Debug)]
pub struct ImagePropertyValue {
    pub key: ImagePropertyKey,
    pub value: String,
    pub has_changed: bool,
}

impl From<ImageProperty> for ImagePropertyValue {
    fn from(property: ImageProperty) -> Self {
        ImagePropertyValue {
            key: property.key,
            value: property.default_value,
            has_changed: false,
        }
    }
}

impl From<ImagePropertyValue> for common::ImagePropertyValue {
    fn from(value: ImagePropertyValue) -> Self {
        common::ImagePropertyValue {
            key: value.key.0,
            value: value.value,
        }
    }
}

impl From<common::ImagePropertyValue> for ImagePropertyValue {
    fn from(value: common::ImagePropertyValue) -> Self {
        ImagePropertyValue {
            key: ImagePropertyKey(value.key),
            value: value.value,
            has_changed: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumUiType"]
pub enum UiType {
    Switch,
    Text,
    Password,
    Enum,
}

impl From<UiType> for common::UiType {
    fn from(ui_type: UiType) -> Self {
        match ui_type {
            UiType::Switch => common::UiType::Switch,
            UiType::Text => common::UiType::Text,
            UiType::Password => common::UiType::Password,
            UiType::Enum => common::UiType::Enum,
        }
    }
}

impl From<common::UiType> for Option<UiType> {
    fn from(ui_type: common::UiType) -> Self {
        match ui_type {
            common::UiType::Unspecified => None,
            common::UiType::Switch => Some(UiType::Switch),
            common::UiType::Text => Some(UiType::Text),
            common::UiType::Password => Some(UiType::Password),
            common::UiType::Enum => Some(UiType::Enum),
        }
    }
}
