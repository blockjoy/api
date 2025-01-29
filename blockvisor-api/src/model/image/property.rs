use std::collections::{HashMap, HashSet};

use derive_more::{Deref, Display, From, Into};
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
use crate::util::LOWER_KEBAB_CASE;

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
    /// Multiple defaults set for image property key group `{0}`.
    GroupMultipleDefaults(ImagePropertyGroup),
    /// No default set for image property key group `{0}`.
    GroupNoDefault(ImagePropertyGroup),
    /// ImagePropertyGroup is not lower-kebab-case: {0}
    PropertyGroupChars(String),
    /// ImagePropertyGroup must be at least 3 characters: {0}
    PropertyGroupLen(String),
    /// ImagePropertyKey is not lower-kebab-case: {0}
    PropertyKeyChars(String),
    /// ImagePropertyKey must be at least 3 characters: {0}
    PropertyKeyLen(String),
    /// Unknown UiType.
    UnknownUiType,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ById(_, NotFound) => Status::not_found("Image property ot found."),
            GroupMultipleDefaults(_) | GroupNoDefault(_) => {
                Status::failed_precondition("is_group_default")
            }
            UnknownUiType => Status::invalid_argument("ui_type"),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From)]
pub struct ImagePropertyId(Uuid);

#[derive(Clone, derive_more::Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, Into)]
#[debug("{_0}")]
pub struct ImagePropertyKey(String);

impl ImagePropertyKey {
    pub fn new(key: String) -> Result<Self, Error> {
        if key.len() < 3 {
            Err(Error::PropertyKeyLen(key))
        } else if !key.chars().all(|c| LOWER_KEBAB_CASE.contains(c)) {
            Err(Error::PropertyKeyChars(key))
        } else {
            Ok(ImagePropertyKey(key))
        }
    }
}

#[derive(Clone, derive_more::Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, Into)]
#[debug("{_0}")]
pub struct ImagePropertyGroup(String);

impl ImagePropertyGroup {
    pub fn new(key: String) -> Result<Self, Error> {
        if key.len() < 3 {
            Err(Error::PropertyGroupLen(key))
        } else if !key.chars().all(|c| LOWER_KEBAB_CASE.contains(c)) {
            Err(Error::PropertyGroupChars(key))
        } else {
            Ok(ImagePropertyGroup(key))
        }
    }
}

#[derive(Clone, Debug, Queryable)]
#[diesel(table_name = image_properties)]
pub struct ImageProperty {
    pub id: ImagePropertyId,
    pub image_id: ImageId,
    pub key: ImagePropertyKey,
    pub key_group: Option<ImagePropertyGroup>,
    pub is_group_default: Option<bool>,
    pub new_archive: bool,
    pub default_value: String,
    pub dynamic_value: bool,
    pub description: Option<String>,
    pub ui_type: UiType,
    pub add_cpu_cores: Option<i64>,
    pub add_memory_bytes: Option<i64>,
    pub add_disk_bytes: Option<i64>,
    pub display_name: Option<String>,
    pub display_group: Option<String>,
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
            key: property.key.into(),
            key_group: property.key_group.map(Into::into),
            is_group_default: property.is_group_default,
            new_archive: property.new_archive,
            default_value: property.default_value,
            dynamic_value: property.dynamic_value,
            ui_type: common::UiType::from(property.ui_type).into(),
            display_name: property.display_name,
            display_group: property.display_group,
            description: property.description,
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
    pub key_group: Option<ImagePropertyGroup>,
    pub is_group_default: Option<bool>,
    pub new_archive: bool,
    pub default_value: String,
    pub dynamic_value: bool,
    pub description: Option<String>,
    pub ui_type: UiType,
    pub add_cpu_cores: Option<i64>,
    pub add_memory_bytes: Option<i64>,
    pub add_disk_bytes: Option<i64>,
    pub display_name: Option<String>,
    pub display_group: Option<String>,
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
            display_name: property.display_name,
            display_group: property.display_group,
        }
    }

    pub fn from(image_id: ImageId, property: api::AddImageProperty) -> Result<Self, Error> {
        let ui_type = property.ui_type().try_into()?;

        Ok(NewProperty {
            image_id,
            key: ImagePropertyKey::new(property.key)?,
            key_group: property
                .key_group
                .map(ImagePropertyGroup::new)
                .transpose()?,
            is_group_default: property.is_group_default,
            new_archive: property.new_archive,
            default_value: property.default_value,
            dynamic_value: property.dynamic_value,
            description: property.description,
            ui_type,
            add_cpu_cores: property.add_cpu_cores,
            add_memory_bytes: property.add_memory_bytes,
            add_disk_bytes: property.add_disk_bytes,
            display_name: property.display_name,
            display_group: property.display_group,
        })
    }

    pub async fn bulk_create(
        properties: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<ImageProperty>, Error> {
        let mut groups = HashSet::new();
        let mut defaults = HashSet::new();
        for property in &properties {
            if let Some(group) = &property.key_group {
                groups.insert(group.clone());
                if property.is_group_default.unwrap_or(false) && !defaults.insert(group.clone()) {
                    return Err(Error::GroupMultipleDefaults(group.clone()));
                }
            }
        }

        for group in groups {
            if !defaults.contains(&group) {
                return Err(Error::GroupNoDefault(group));
            }
        }

        diesel::insert_into(image_properties::table)
            .values(properties)
            .get_results(conn)
            .await
            .map_err(Error::BulkCreate)
    }
}

#[derive(Clone, Debug)]
pub struct NewImagePropertyValue {
    pub key: ImagePropertyKey,
    pub value: String,
    pub has_changed: bool,
}

impl From<ImageProperty> for NewImagePropertyValue {
    fn from(property: ImageProperty) -> Self {
        NewImagePropertyValue {
            key: property.key,
            value: property.default_value,
            has_changed: false,
        }
    }
}

impl From<NewImagePropertyValue> for api::NewImagePropertyValue {
    fn from(value: NewImagePropertyValue) -> Self {
        api::NewImagePropertyValue {
            key: value.key.0,
            value: value.value,
        }
    }
}

impl TryFrom<api::NewImagePropertyValue> for NewImagePropertyValue {
    type Error = Error;

    fn try_from(value: api::NewImagePropertyValue) -> Result<Self, Self::Error> {
        Ok(NewImagePropertyValue {
            key: ImagePropertyKey::new(value.key)?,
            value: value.value,
            has_changed: true,
        })
    }
}

impl From<PropertyValueConfig> for NewImagePropertyValue {
    fn from(property: PropertyValueConfig) -> Self {
        NewImagePropertyValue {
            key: property.key,
            value: property.value,
            has_changed: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PropertyValueConfig {
    pub key: ImagePropertyKey,
    pub key_group: Option<ImagePropertyGroup>,
    pub value: String,
    pub has_changed: bool,
}

impl From<ImageProperty> for PropertyValueConfig {
    fn from(property: ImageProperty) -> Self {
        PropertyValueConfig {
            key: property.key,
            key_group: property.key_group,
            value: property.default_value,
            has_changed: false,
        }
    }
}

impl From<PropertyValueConfig> for common::PropertyValueConfig {
    fn from(value: PropertyValueConfig) -> Self {
        common::PropertyValueConfig {
            key: value.key.0,
            key_group: value.key_group.map(|group| group.0),
            value: value.value,
        }
    }
}

impl TryFrom<common::PropertyValueConfig> for PropertyValueConfig {
    type Error = Error;

    fn try_from(value: common::PropertyValueConfig) -> Result<Self, Self::Error> {
        Ok(PropertyValueConfig {
            key: ImagePropertyKey::new(value.key)?,
            key_group: value.key_group.map(ImagePropertyGroup::new).transpose()?,
            value: value.value,
            has_changed: true,
        })
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

impl TryFrom<common::UiType> for UiType {
    type Error = Error;

    fn try_from(ui_type: common::UiType) -> Result<Self, Self::Error> {
        match ui_type {
            common::UiType::Unspecified => Err(Error::UnknownUiType),
            common::UiType::Switch => Ok(UiType::Switch),
            common::UiType::Text => Ok(UiType::Text),
            common::UiType::Password => Ok(UiType::Password),
            common::UiType::Enum => Ok(UiType::Enum),
        }
    }
}

pub struct PropertyMap {
    pub key_to_value: HashMap<ImagePropertyKey, NewImagePropertyValue>,
    pub key_to_group: HashMap<ImagePropertyKey, ImagePropertyGroup>,
    pub group_to_keys: HashMap<ImagePropertyGroup, Vec<ImagePropertyKey>>,
}

impl PropertyMap {
    pub fn new(properties: Vec<ImageProperty>) -> Self {
        let mut key_to_value = HashMap::new();
        let mut key_to_group = HashMap::new();
        let mut group_to_keys = HashMap::<_, Vec<_>>::new();

        for property in properties {
            if let Some(group) = &property.key_group {
                key_to_group.insert(property.key.clone(), group.clone());
                group_to_keys
                    .entry(group.clone())
                    .or_default()
                    .push(property.key.clone());

                if property.is_group_default == Some(true) {
                    key_to_value.insert(property.key.clone(), property.into());
                }
            } else {
                key_to_value.insert(property.key.clone(), property.into());
            }
        }

        PropertyMap {
            key_to_value,
            key_to_group,
            group_to_keys,
        }
    }

    pub fn apply_overrides(
        mut self,
        overrides: Vec<NewImagePropertyValue>,
    ) -> Vec<PropertyValueConfig> {
        for value in overrides {
            if let Some(group) = self.key_to_group.get(&value.key) {
                if let Some(keys) = self.group_to_keys.get(group) {
                    for key in keys {
                        self.key_to_value.remove(key);
                    }
                }
            }

            self.key_to_value.insert(value.key.clone(), value);
        }

        self.key_to_value
            .into_values()
            .map(|new_value| {
                let key_group = self.key_to_group.get(&new_value.key).cloned();
                PropertyValueConfig {
                    key: new_value.key,
                    key_group,
                    value: new_value.value,
                    has_changed: new_value.has_changed,
                }
            })
            .collect()
    }
}
