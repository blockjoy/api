use std::cmp::max;
use std::collections::HashSet;

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr, IntoIterator};
use diesel::deserialize::{FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::sql_types::Jsonb;
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel::serialize::{Output, ToSql};
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use prost::Message;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::{OrgId, Resource, ResourceId, ResourceType};
use crate::auth::AuthZ;
use crate::database::Conn;
use crate::grpc::{common, Status};
use crate::model::image::property::{ImageProperty, ImagePropertyKey};
use crate::model::image::Image;
use crate::model::schema::{configs, sql_types};
use crate::store::StoreId;
use crate::util::HashVec;

use super::property::ImagePropertyValue;
use super::rule::{FirewallAction, FirewallRule};
use super::{Archive, ArchiveId, ImageId, ImageRule};

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Image archive error: {0}
    Archive(#[from] super::archive::Error),
    /// Failed to get image config for id {0}: {1}
    ById(ConfigId, diesel::result::Error),
    /// Failed to get image config for ids {0:?}: {1}
    ByIds(HashSet<ConfigId>, diesel::result::Error),
    /// Failed to find property `{0}` to change.
    ChangeProperty(ImagePropertyKey),
    /// Failed to create new image Config: {0}
    Create(diesel::result::Error),
    /// Failed to decode NodeConfig proto bytes: {0}
    DecodeNodeConfig(prost::DecodeError),
    /// Missing FirewallConfig. This should not happen.
    MissingFirewallConfig,
    /// Missing ImageConfig. This should not happen.
    MissingImageConfig,
    /// Missing VmConfig. This should not happen.
    MissingVmConfig,
    /// Failed to parse ArchiveId: {0}
    ParseArchiveId(uuid::Error),
    /// Failed to parse ImageId: {0}
    ParseImageId(uuid::Error),
    /// Image config property error: {0}
    Property(#[from] super::property::Error),
    /// Image config firewall rule error: {0}
    Rule(#[from] super::rule::Error),
    /// Invalid VM cpu_count: {0}
    VmCpu(std::num::TryFromIntError),
    /// Invalid VM disk bytes: {0}
    VmDisk(std::num::TryFromIntError),
    /// Invalid VM memory bytes: {0}
    VmMemory(std::num::TryFromIntError),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ById(_, NotFound) => Status::not_found("Not found."),
            // safety: `key` is an input from the client
            ChangeProperty(key) => Status::not_found(format!("property.key: {key}")),
            ParseArchiveId(_) => Status::invalid_argument("archive_id"),
            ParseImageId(_) => Status::invalid_argument("image_id"),
            ById(_, _)
            | ByIds(_, _)
            | Create(_)
            | DecodeNodeConfig(_)
            | MissingImageConfig
            | MissingFirewallConfig
            | MissingVmConfig
            | VmCpu(_)
            | VmDisk(_)
            | VmMemory(_) => Status::internal("Internal error."),
            Archive(err) => err.into(),
            Property(err) => err.into(),
            Rule(err) => err.into(),
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    DieselNewType,
    Deref,
    From,
    FromStr,
    Serialize,
    Deserialize,
)]
pub struct ConfigId(Uuid);

#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumConfigType"]
pub enum ConfigType {
    Legacy,
    Node,
}

#[derive(Clone, Debug, Queryable)]
#[diesel(table_name = configs)]
pub struct Config {
    pub id: ConfigId,
    pub image_id: ImageId,
    pub archive_id: ArchiveId,
    pub config_type: ConfigType,
    pub config: ConfigBytes,
    pub created_by_type: ResourceType,
    pub created_by_id: ResourceId,
    pub created_at: DateTime<Utc>,
}

impl Config {
    pub async fn by_id(id: ConfigId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        configs::table
            .filter(configs::id.eq(id))
            .get_result(conn)
            .await
            .map_err(|err| Error::ById(id, err))
    }

    pub async fn by_ids(ids: &HashSet<ConfigId>, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        configs::table
            .filter(configs::id.eq_any(ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByIds(ids.clone(), err))
    }

    pub fn node_config(&self) -> Result<NodeConfig, Error> {
        match self.config_type {
            ConfigType::Node => (&self.config).try_into(),
            ConfigType::Legacy => Ok(NodeConfig::legacy()),
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = configs)]
pub struct NewConfig {
    pub image_id: ImageId,
    pub archive_id: ArchiveId,
    pub config_type: ConfigType,
    pub config: ConfigBytes,
}

impl NewConfig {
    pub async fn create(self, authz: &AuthZ, conn: &mut Conn<'_>) -> Result<Config, Error> {
        let created_by = Resource::from(authz);
        diesel::insert_into(configs::table)
            .values((
                self,
                configs::created_by_type.eq(created_by.typ()),
                configs::created_by_id.eq(created_by.id()),
                configs::created_at.eq(Utc::now()),
            ))
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}

#[derive(Clone, Debug, Deref, DieselNewType)]
pub struct ConfigBytes(Vec<u8>);

pub struct NodeConfig {
    pub vm: VmConfig,
    pub image: ImageConfig,
    pub firewall: FirewallConfig,
}

impl NodeConfig {
    pub async fn new(
        image: Image,
        org_id: Option<OrgId>,
        new_values: Vec<ImagePropertyValue>,
        add_rules: Vec<FirewallRule>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let values = ImageProperty::by_image_id(image.id, conn)
            .await?
            .into_iter()
            .map(ImagePropertyValue::from)
            .chain(new_values)
            .collect();
        let rules = ImageRule::by_image_id(image.id, conn)
            .await?
            .into_iter()
            .map(FirewallRule::from)
            .chain(add_rules)
            .collect();

        Self::generate_from(image, org_id, values, rules, conn).await
    }

    pub async fn upgrade(
        self,
        image: Image,
        org_id: Option<OrgId>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let old_values = ImageProperty::by_image_id(self.image.image_id, conn)
            .await?
            .into_iter()
            .to_map_keep_last(|property| (property.key, property.default_value));
        let changed_values = self.image.values.into_iter().filter(|property| {
            if let Some(default) = old_values.get(&property.key) {
                property.value != *default
            } else {
                true
            }
        });
        let new_values = ImageProperty::by_image_id(image.id, conn)
            .await?
            .into_iter()
            .map(ImagePropertyValue::from)
            .chain(changed_values)
            .collect();

        let old_rules = ImageRule::by_image_id(self.image.image_id, conn)
            .await?
            .into_iter()
            .to_map_keep_last(|rule| (rule.key.clone(), FirewallRule::from(rule)));
        let changed_rules = self.firewall.rules.into_iter().filter(|rule| {
            if let Some(default) = old_rules.get(&rule.key) {
                rule != default
            } else {
                true
            }
        });
        let new_rules = ImageRule::by_image_id(image.id, conn)
            .await?
            .into_iter()
            .map(FirewallRule::from)
            .chain(changed_rules)
            .collect();

        Self::generate_from(image, org_id, new_values, new_rules, conn).await
    }

    /// Generate a `NodeConfig` from image property values and firewall rules.
    ///
    /// This will find the `Archive` for the changed set of `image_property_ids`
    /// where `new_archive == true`.
    ///
    /// The required resources are calculated by adding per-property resources
    /// to the minimum requirements defined in the `Image`.
    async fn generate_from(
        image: Image,
        org_id: Option<OrgId>,
        values: Vec<ImagePropertyValue>,
        rules: Vec<FirewallRule>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let changed_keys: HashSet<_> = values
            .iter()
            .filter_map(|value| value.has_changed.then_some(&value.key))
            .collect();
        let changed_properties: Vec<_> = ImageProperty::by_image_id(image.id, conn)
            .await?
            .into_iter()
            .filter(|property| changed_keys.contains(&property.key))
            .collect();
        let new_archive_ids: Vec<_> = changed_properties
            .iter()
            .filter_map(|property| property.new_archive.then_some(property.id))
            .collect();
        let archive = Archive::by_property_ids(image.id, org_id, new_archive_ids, conn).await?;

        let (cpu, mem, disk) = changed_properties.iter().fold(
            (
                image.min_cpu_cores,
                image.min_memory_bytes,
                image.min_disk_bytes,
            ),
            |acc, prop| {
                (
                    acc.0 + prop.add_cpu_cores.unwrap_or(0),
                    acc.1 + prop.add_memory_bytes.unwrap_or(0),
                    acc.2 + prop.add_disk_bytes.unwrap_or(0),
                )
            },
        );
        let (cpu_cores, memory_bytes, disk_bytes) = (
            u64::try_from(max(cpu, image.min_cpu_cores)).map_err(Error::VmCpu)?,
            u64::try_from(max(mem, image.min_memory_bytes)).map_err(Error::VmMemory)?,
            u64::try_from(max(disk, image.min_disk_bytes)).map_err(Error::VmDisk)?,
        );

        Ok(NodeConfig {
            vm: VmConfig {
                cpu_cores,
                memory_bytes,
                disk_bytes,
                ramdisks: image.ramdisks,
            },
            image: ImageConfig {
                image_id: image.id,
                image_uri: image.image_uri,
                archive_id: archive.id,
                store_id: archive.store_id,
                values,
            },
            firewall: FirewallConfig {
                default_in: image.default_firewall_in,
                default_out: image.default_firewall_out,
                rules,
            },
        })
    }

    fn legacy() -> Self {
        NodeConfig {
            vm: VmConfig {
                cpu_cores: 0,
                memory_bytes: 0,
                disk_bytes: 0,
                ramdisks: Ramdisks(vec![]),
            },
            image: ImageConfig {
                image_id: Uuid::nil().into(),
                image_uri: "legacy".to_string(),
                archive_id: Uuid::nil().into(),
                store_id: "legacy".to_string().into(),
                values: vec![],
            },
            firewall: FirewallConfig {
                default_in: FirewallAction::Drop,
                default_out: FirewallAction::Allow,
                rules: vec![],
            },
        }
    }
}

impl From<NodeConfig> for ConfigBytes {
    fn from(config: NodeConfig) -> Self {
        ConfigBytes(common::NodeConfig::from(config).encode_to_vec())
    }
}

impl TryFrom<&ConfigBytes> for NodeConfig {
    type Error = Error;

    fn try_from(config: &ConfigBytes) -> Result<Self, Self::Error> {
        common::NodeConfig::decode(&***config)
            .map_err(Error::DecodeNodeConfig)?
            .try_into()
    }
}

impl From<NodeConfig> for common::NodeConfig {
    fn from(config: NodeConfig) -> Self {
        common::NodeConfig {
            vm: Some(config.vm.into()),
            image: Some(config.image.into()),
            firewall: Some(config.firewall.into()),
        }
    }
}

impl TryFrom<common::NodeConfig> for NodeConfig {
    type Error = Error;

    fn try_from(config: common::NodeConfig) -> Result<Self, Self::Error> {
        let vm = config.vm.ok_or(Error::MissingVmConfig)?;
        let image = config.image.ok_or(Error::MissingImageConfig)?;
        let firewall = config.firewall.ok_or(Error::MissingFirewallConfig)?;

        Ok(NodeConfig {
            vm: vm.into(),
            image: image.try_into()?,
            firewall: firewall.try_into()?,
        })
    }
}

pub struct VmConfig {
    pub cpu_cores: u64,
    pub memory_bytes: u64,
    pub disk_bytes: u64,
    pub ramdisks: Ramdisks,
}

impl From<VmConfig> for common::VmConfig {
    fn from(config: VmConfig) -> Self {
        common::VmConfig {
            cpu_cores: config.cpu_cores,
            memory_bytes: config.memory_bytes,
            disk_bytes: config.disk_bytes,
            ramdisks: config.ramdisks.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<common::VmConfig> for VmConfig {
    fn from(config: common::VmConfig) -> Self {
        VmConfig {
            cpu_cores: config.cpu_cores,
            memory_bytes: config.memory_bytes,
            disk_bytes: config.disk_bytes,
            ramdisks: Ramdisks(config.ramdisks.into_iter().map(Into::into).collect()),
        }
    }
}

#[derive(Clone, Debug, AsExpression, From, FromSqlRow, IntoIterator, Serialize, Deserialize)]
#[diesel(sql_type = Jsonb)]
pub struct Ramdisks(pub Vec<RamdiskConfig>);

impl FromSql<Jsonb, Pg> for Ramdisks {
    fn from_sql(value: PgValue<'_>) -> diesel::deserialize::Result<Self> {
        serde_json::from_value(FromSql::<Jsonb, Pg>::from_sql(value)?).map_err(Into::into)
    }
}

impl ToSql<Jsonb, Pg> for Ramdisks {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        let json = serde_json::to_value(self).unwrap();
        <serde_json::Value as ToSql<Jsonb, Pg>>::to_sql(&json, &mut out.reborrow())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RamdiskConfig {
    pub mount: String,
    pub size_bytes: u64,
}

impl From<RamdiskConfig> for common::RamdiskConfig {
    fn from(config: RamdiskConfig) -> Self {
        common::RamdiskConfig {
            mount: config.mount,
            size_bytes: config.size_bytes,
        }
    }
}

impl From<common::RamdiskConfig> for RamdiskConfig {
    fn from(config: common::RamdiskConfig) -> Self {
        RamdiskConfig {
            mount: config.mount,
            size_bytes: config.size_bytes,
        }
    }
}

pub struct ImageConfig {
    pub image_id: ImageId,
    pub image_uri: String,
    pub archive_id: ArchiveId,
    pub store_id: StoreId,
    pub values: Vec<ImagePropertyValue>,
}

impl From<ImageConfig> for common::ImageConfig {
    fn from(config: ImageConfig) -> Self {
        common::ImageConfig {
            image_id: config.image_id.to_string(),
            image_uri: config.image_uri,
            archive_id: config.archive_id.to_string(),
            store_id: config.store_id.to_string(),
            values: config.values.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<common::ImageConfig> for ImageConfig {
    type Error = Error;

    fn try_from(config: common::ImageConfig) -> Result<Self, Self::Error> {
        Ok(ImageConfig {
            image_id: config.image_id.parse().map_err(Error::ParseImageId)?,
            image_uri: config.image_uri,
            archive_id: config.archive_id.parse().map_err(Error::ParseArchiveId)?,
            store_id: config.store_id.into(),
            values: config.values.into_iter().map(Into::into).collect(),
        })
    }
}

pub struct FirewallConfig {
    pub default_in: FirewallAction,
    pub default_out: FirewallAction,
    pub rules: Vec<FirewallRule>,
}

impl From<FirewallConfig> for common::FirewallConfig {
    fn from(config: FirewallConfig) -> Self {
        common::FirewallConfig {
            default_in: common::FirewallAction::from(config.default_in) as i32,
            default_out: common::FirewallAction::from(config.default_out) as i32,
            rules: config.rules.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<common::FirewallConfig> for FirewallConfig {
    type Error = Error;

    fn try_from(config: common::FirewallConfig) -> Result<Self, Self::Error> {
        Ok(FirewallConfig {
            default_in: config.default_in().try_into()?,
            default_out: config.default_out().try_into()?,
            rules: config
                .rules
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}
