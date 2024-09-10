use std::collections::HashSet;
use std::{fmt, str};

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr, Into};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use tonic::Status;
use uuid::Uuid;

use crate::auth::resource::OrgId;
use crate::auth::AuthZ;
use crate::database::Conn;
use crate::grpc::{api, common};
use crate::model::schema::protocol_versions;
use crate::model::Region;
use crate::util::sql::Version;
use crate::util::{NanosUtc, LOWER_KEBAB_CASE};

use super::{ProtocolId, Visibility};

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to create protocol version: {0}
    Create(diesel::result::Error),
    /// No versions found for version key.
    NoVersions,
    /// Protocol version model error: {0}
    Protocol(#[from] super::Error),
    /// Failed to find protocol versions for protocol id `{0:?}`: {1}
    ByProtocolId(ProtocolId, diesel::result::Error),
    /// Failed to find protocol versions for protocol ids `{0:?}`: {1}
    ByProtocolIds(HashSet<ProtocolId>, diesel::result::Error),
    /// Failed to find protocol version for id `{0:?}`: {1}
    ById(VersionId, diesel::result::Error),
    /// Failed to find protocol version ids `{0:?}`: {1}
    ByIds(HashSet<VersionId>, diesel::result::Error),
    /// Failed to find protocol versions for key `{0}`: {1}
    ByKey(VersionKey, diesel::result::Error),
    /// Failed to parse VersionKey `{0}` into 2 parts delimited by `/`.
    KeyParts(String),
    /// Invalid characters found in `version_key.protocol_key`: {0}
    ProtocolKeyChars(String),
    /// Protocol key must be at least 3 characters: {0}
    ProtocolKeyLen(String),
    /// Failed to update protocol version id {0}: {1}
    Update(VersionId, diesel::result::Error),
    /// Variant key must be at least 3 characters: {0}
    VariantKeyLen(String),
    /// Invalid characters found in `version_key.variant_key`: {0}
    VariantKeyChars(String),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            ById(_, NotFound) | ByIds(_, NotFound) | ByKey(_, NotFound) | NoVersions => {
                Status::not_found("Not found.")
            }
            ProtocolKeyChars(_) | ProtocolKeyLen(_) => {
                Status::invalid_argument("version_key.protocol_key")
            }
            VariantKeyChars(_) | VariantKeyLen(_) => {
                Status::invalid_argument("version_key.variant_key")
            }
            Protocol(err) => err.into(),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct VersionId(Uuid);

#[derive(Clone, Debug, Insertable, Queryable, Selectable)]
#[diesel(table_name = protocol_versions)]
pub struct ProtocolVersion {
    pub id: VersionId,
    pub org_id: Option<OrgId>,
    pub protocol_id: ProtocolId,
    pub protocol_key: ProtocolKey,
    pub variant_key: VariantKey,
    pub semantic_version: Version,
    pub sku_code: String,
    pub description: Option<String>,
    pub visibility: Visibility,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl ProtocolVersion {
    pub async fn by_id(
        id: VersionId,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        protocol_versions::table
            .find(id)
            .filter(
                protocol_versions::org_id
                    .eq(org_id)
                    .or(protocol_versions::org_id.is_null()),
            )
            .filter(protocol_versions::visibility.eq_any(<&[Visibility]>::from(authz)))
            .get_result(conn)
            .await
            .map_err(|err| Error::ById(id, err))
    }

    pub async fn by_ids(
        ids: &HashSet<VersionId>,
        org_ids: &HashSet<OrgId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        protocol_versions::table
            .filter(protocol_versions::id.eq_any(ids))
            .filter(protocol_versions::visibility.eq_any(<&[Visibility]>::from(authz)))
            .filter(
                protocol_versions::org_id
                    .eq_any(org_ids)
                    .or(protocol_versions::org_id.is_null()),
            )
            .get_results(conn)
            .await
            .map_err(|err| Error::ByIds(ids.clone(), err))
    }

    pub async fn by_key(
        version_key: &VersionKey,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let mut versions: Vec<Self> = protocol_versions::table
            .filter(protocol_versions::protocol_key.eq(&version_key.protocol_key))
            .filter(protocol_versions::variant_key.eq(&version_key.variant_key))
            .filter(
                protocol_versions::org_id
                    .eq(org_id)
                    .or(protocol_versions::org_id.is_null()),
            )
            .filter(protocol_versions::visibility.eq_any(<&[Visibility]>::from(authz)))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByKey(version_key.clone(), err))?;

        versions.sort_by_cached_key(|version| version.semantic_version.clone());
        Ok(versions)
    }

    pub async fn latest_by_key(
        version_key: &VersionKey,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let mut versions = Self::by_key(version_key, org_id, authz, conn).await?;
        if let Some(version) = versions.pop() {
            Ok(version)
        } else {
            Err(Error::NoVersions)
        }
    }

    pub async fn by_protocol_id(
        protocol_id: ProtocolId,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        protocol_versions::table
            .filter(protocol_versions::protocol_id.eq(protocol_id))
            .filter(
                protocol_versions::org_id
                    .eq(org_id)
                    .or(protocol_versions::org_id.is_null()),
            )
            .filter(protocol_versions::visibility.eq_any(<&[Visibility]>::from(authz)))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByProtocolId(protocol_id, err))
    }

    pub async fn by_protocol_ids(
        protocol_ids: &HashSet<ProtocolId>,
        org_ids: &HashSet<OrgId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        protocol_versions::table
            .filter(protocol_versions::protocol_id.eq_any(protocol_ids))
            .filter(
                protocol_versions::org_id
                    .eq_any(org_ids)
                    .or(protocol_versions::org_id.is_null()),
            )
            .filter(protocol_versions::visibility.eq_any(<&[Visibility]>::from(authz)))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByProtocolIds(protocol_ids.clone(), err))
    }

    /// The Stock Keeping Unit identifier.
    ///
    /// Example format: FMN-BLASTGETH-A-MN-USW1-USD-M
    /// where:
    ///   FMN - hardcoded for Nodes (Fully-Managed Node)
    ///   BLASTGETH-A - Node ticker (Blast Geth Archive)
    ///   A - Node Type (archive)
    ///   MN - Net type (mainnet)
    ///   USW1 - Region (US west)
    ///   USD - hardcoded for now
    ///   M - Billing cycle (monthly)
    pub fn sku(&self, region: &Region) -> Option<String> {
        let sku_code = &self.sku_code;
        region
            .pricing_tier
            .as_deref()
            .map(|tier| format!("FMN-{sku_code}-{tier}-USD-M"))
    }
}

impl From<ProtocolVersion> for api::ProtocolVersion {
    fn from(version: ProtocolVersion) -> Self {
        api::ProtocolVersion {
            protocol_version_id: version.id.to_string(),
            org_id: version.org_id.map(|id| id.to_string()),
            protocol_id: version.protocol_id.to_string(),
            version_key: Some(common::ProtocolVersionKey {
                protocol_key: version.protocol_key.into(),
                variant_key: version.variant_key.into(),
            }),
            semantic_version: version.semantic_version.to_string(),
            sku_code: version.sku_code,
            description: version.description,
            visibility: common::Visibility::from(version.visibility).into(),
            created_at: Some(NanosUtc::from(version.created_at).into()),
            updated_at: version.updated_at.map(NanosUtc::from).map(Into::into),
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = protocol_versions)]
pub struct NewVersion<'v> {
    pub org_id: Option<OrgId>,
    pub protocol_id: ProtocolId,
    pub protocol_key: ProtocolKey,
    pub variant_key: VariantKey,
    pub semantic_version: &'v Version,
    pub sku_code: &'v str,
    pub description: Option<String>,
}

impl<'v> NewVersion<'v> {
    pub async fn create(self, conn: &mut Conn<'_>) -> Result<ProtocolVersion, Error> {
        diesel::insert_into(protocol_versions::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = protocol_versions)]
pub struct UpdateVersion<'u> {
    pub id: VersionId,
    pub sku_code: Option<&'u str>,
    pub description: Option<&'u str>,
    pub visibility: Option<Visibility>,
}

impl<'u> UpdateVersion<'u> {
    pub async fn apply(self, conn: &mut Conn<'_>) -> Result<ProtocolVersion, Error> {
        let id = self.id;
        diesel::update(protocol_versions::table.find(id))
            .set((self, protocol_versions::updated_at.eq(Utc::now())))
            .get_result(conn)
            .await
            .map_err(|err| Error::Update(id, err))
    }
}

// A key identifier to a specific protocol.
#[derive(Clone, Debug, Display, PartialEq, Eq, DieselNewType, Deref, Into)]
pub struct ProtocolKey(String);

impl ProtocolKey {
    pub fn new(key: String) -> Result<Self, Error> {
        if key.len() < 3 {
            Err(Error::ProtocolKeyLen(key))
        } else if !key.chars().all(|c| LOWER_KEBAB_CASE.contains(c)) {
            Err(Error::ProtocolKeyChars(key))
        } else {
            Ok(ProtocolKey(key))
        }
    }
}

// A key identifier to a protocol variant.
#[derive(Clone, Debug, Display, PartialEq, Eq, DieselNewType, Deref, Into)]
pub struct VariantKey(String);

impl VariantKey {
    pub fn new(key: String) -> Result<Self, Error> {
        if key.len() < 3 {
            Err(Error::VariantKeyLen(key))
        } else if !key.chars().all(|c| LOWER_KEBAB_CASE.contains(c)) {
            Err(Error::VariantKeyChars(key))
        } else {
            Ok(VariantKey(key))
        }
    }
}

// A key identifier to some protocol version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionKey {
    pub protocol_key: ProtocolKey,
    pub variant_key: VariantKey,
}

impl VersionKey {
    pub const fn new(protocol_key: ProtocolKey, variant_key: VariantKey) -> Self {
        VersionKey {
            protocol_key,
            variant_key,
        }
    }
}

impl From<VersionKey> for common::ProtocolVersionKey {
    fn from(key: VersionKey) -> Self {
        common::ProtocolVersionKey {
            protocol_key: key.protocol_key.into(),
            variant_key: key.variant_key.into(),
        }
    }
}

impl TryFrom<common::ProtocolVersionKey> for VersionKey {
    type Error = Error;

    fn try_from(key: common::ProtocolVersionKey) -> Result<Self, Self::Error> {
        let protocol_key = ProtocolKey::new(key.protocol_key)?;
        let variant_key = VariantKey::new(key.variant_key)?;
        Ok(VersionKey::new(protocol_key, variant_key))
    }
}

impl fmt::Display for VersionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.protocol_key, self.variant_key)
    }
}

impl str::FromStr for VersionKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(Error::KeyParts(s.to_string()));
        }

        let protocol_key = ProtocolKey::new(parts[0].to_string())?;
        let variant_key = VariantKey::new(parts[1].to_string())?;
        Ok(VersionKey::new(protocol_key, variant_key))
    }
}
