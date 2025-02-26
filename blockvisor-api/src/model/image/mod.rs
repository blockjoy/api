pub mod archive;

pub use archive::{Archive, ArchiveId};

pub mod config;
pub use config::{Config, ConfigId, NewConfig, NodeConfig};

pub mod property;
pub use property::{ImageProperty, ImagePropertyId, NewProperty, UiType};

pub mod rule;
pub use rule::{FirewallRule, ImageRule, ImageRuleId, NewImageRule};

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::auth::AuthZ;
use crate::auth::resource::OrgId;
use crate::database::{ReadConn, WriteConn};
use crate::grpc::Status;
use crate::model::protocol::{VersionId, Visibility};
use crate::model::schema::images;
use crate::model::sql::Version;

use self::config::Ramdisks;
use self::rule::FirewallAction;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to find image for protocol version `{0}` (org: {1:?}), build: {2}: {3}
    ByBuild(VersionId, Option<OrgId>, i64, diesel::result::Error),
    /// Failed to find image id `{0}`: {1}
    ById(ImageId, diesel::result::Error),
    /// Failed to find image for protocol version `{0}` (org: {1:?}): {2}
    ByVersion(VersionId, Option<OrgId>, diesel::result::Error),
    /// Failed to find image for protocol versions `{0:?}` (org: {1:?}): {2}
    ByVersions(HashSet<VersionId>, Option<OrgId>, diesel::result::Error),
    /// Failed to create image: {0}
    Create(diesel::result::Error),
    /// Failed to get the last build for protocol version `{0}`: {1}
    LatestBuild(VersionId, diesel::result::Error),
    /// Failed to update image id {0}: {1}
    Update(ImageId, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ById(_, NotFound) => Status::not_found("Image not found."),
            ByBuild(_, _, _, NotFound) => Status::not_found("No image for that build."),
            Update(_, NotFound) => Status::not_found("No image updated."),
            _ => Status::internal("Internal error."),
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
pub struct ImageId(Uuid);

#[derive(Clone, Debug, Queryable)]
pub struct Image {
    pub id: ImageId,
    pub org_id: Option<OrgId>,
    pub protocol_version_id: VersionId,
    pub image_uri: String,
    pub build_version: i64,
    pub description: Option<String>,
    pub min_cpu_cores: i64,
    pub min_memory_bytes: i64,
    pub min_disk_bytes: i64,
    pub ramdisks: Ramdisks,
    pub default_firewall_in: FirewallAction,
    pub default_firewall_out: FirewallAction,
    pub visibility: Visibility,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub min_babel_version: Version,
    pub dns_scheme: Option<String>,
}

impl Image {
    pub async fn by_id(
        id: ImageId,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut ReadConn<'_, '_>,
    ) -> Result<Self, Error> {
        images::table
            .find(id)
            .filter(images::org_id.eq(org_id).or(images::org_id.is_null()))
            .filter(images::visibility.eq_any(<&[Visibility]>::from(authz)))
            .get_result(conn)
            .await
            .map_err(|err| Error::ById(id, err))
    }

    pub async fn by_version(
        version_id: VersionId,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut ReadConn<'_, '_>,
    ) -> Result<Vec<Self>, Error> {
        images::table
            .filter(images::protocol_version_id.eq(version_id))
            .filter(images::org_id.eq(org_id).or(images::org_id.is_null()))
            .filter(images::visibility.eq_any(<&[Visibility]>::from(authz)))
            .order_by(images::build_version.desc())
            .get_results(conn)
            .await
            .map_err(|err| Error::ByVersion(version_id, org_id, err))
    }

    pub async fn latest_build(
        version_id: VersionId,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut ReadConn<'_, '_>,
    ) -> Result<Option<Self>, Error> {
        images::table
            .filter(images::protocol_version_id.eq(version_id))
            .filter(images::org_id.eq(org_id).or(images::org_id.is_null()))
            .filter(images::visibility.eq_any(<&[Visibility]>::from(authz)))
            .order_by(images::build_version.desc())
            .first(conn)
            .await
            .optional()
            .map_err(|err| Error::LatestBuild(version_id, err))
    }

    pub async fn by_versions(
        version_ids: &HashSet<VersionId>,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut ReadConn<'_, '_>,
    ) -> Result<Vec<Self>, Error> {
        images::table
            .filter(images::protocol_version_id.eq_any(version_ids))
            .filter(images::org_id.eq(org_id).or(images::org_id.is_null()))
            .filter(images::visibility.eq_any(<&[Visibility]>::from(authz)))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByVersions(version_ids.clone(), org_id, err))
    }

    pub async fn by_build(
        version_id: VersionId,
        org_id: Option<OrgId>,
        build: i64,
        authz: &AuthZ,
        conn: &mut ReadConn<'_, '_>,
    ) -> Result<Self, Error> {
        images::table
            .filter(images::protocol_version_id.eq(version_id))
            .filter(images::org_id.eq(org_id).or(images::org_id.is_null()))
            .filter(images::build_version.eq(build))
            .filter(images::visibility.eq_any(<&[Visibility]>::from(authz)))
            .get_result(conn)
            .await
            .map_err(|err| Error::ByBuild(version_id, org_id, build, err))
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = images)]
pub struct NewImage {
    pub protocol_version_id: VersionId,
    pub org_id: Option<OrgId>,
    pub image_uri: String,
    pub build_version: i64,
    pub description: Option<String>,
    pub min_cpu_cores: i64,
    pub min_memory_bytes: i64,
    pub min_disk_bytes: i64,
    pub min_babel_version: Version,
    pub ramdisks: Ramdisks,
    pub default_firewall_in: FirewallAction,
    pub default_firewall_out: FirewallAction,
    pub dns_scheme: Option<String>,
}

impl NewImage {
    pub async fn create(self, conn: &mut WriteConn<'_, '_>) -> Result<Image, Error> {
        diesel::insert_into(images::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = images)]
pub struct UpdateImage {
    pub id: ImageId,
    pub visibility: Option<Visibility>,
}

impl UpdateImage {
    pub async fn update(self, conn: &mut WriteConn<'_, '_>) -> Result<Image, Error> {
        let id = self.id;
        diesel::update(images::table.find(id))
            .set(self)
            .get_result(conn)
            .await
            .map_err(|err| Error::Update(id, err))
    }
}
