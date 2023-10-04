use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use displaydoc::Display;
use thiserror::Error;
use tonic::Status;

use crate::auth::resource::{Resource, ResourceEntry, ResourceId, ResourceType, UserId};
use crate::auth::token::api_key::{BearerSecret, KeyHash, KeyId, Salt, Secret};
use crate::database::{Conn, WriteConn};

use super::schema::{api_keys, sql_types};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create a new api key: {0}
    CreateNew(diesel::result::Error),
    /// Failed to delete api key: {0}
    DeleteKey(diesel::result::Error),
    /// Failed to find api key by id: {0}
    FindById(diesel::result::Error),
    /// Failed to find api keys by user_id: {0}
    FindByUser(diesel::result::Error),
    /// Missing `updated_at`. This should not happen.
    MissingUpdatedAt,
    /// {0} api keys were deleted. This should not happen.
    MultipleKeysDeleted(usize),
    /// No api keys were deleted.
    NoKeysDeleted,
    /// Failed to regenerate a new api key: {0}
    Regenerate(diesel::result::Error),
    /// Unknown ApiResource value: {0}
    UnknownApiResource(i32),
    /// Failed to update api key label: {0}
    UpdateLabel(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            CreateNew(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("Already exists.")
            }
            DeleteKey(NotFound) | FindById(NotFound) | FindByUser(NotFound) | NoKeysDeleted => {
                Status::not_found("Not found.")
            }
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Debug, Queryable)]
pub struct ApiKey {
    pub id: KeyId,
    pub user_id: UserId,
    pub label: String,
    pub key_hash: KeyHash,
    pub key_salt: Salt,
    pub resource: ApiResource,
    pub resource_id: ResourceId,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl ApiKey {
    pub async fn find_by_id(key_id: KeyId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        api_keys::table
            .find(key_id)
            .get_result(conn)
            .await
            .map_err(Error::FindById)
    }

    pub async fn find_by_user(user_id: UserId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        api_keys::table
            .filter(api_keys::user_id.eq(user_id))
            .get_results(conn)
            .await
            .map_err(Error::FindByUser)
    }

    pub async fn regenerate(
        key_id: KeyId,
        key_hash: KeyHash,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        diesel::update(api_keys::table)
            .filter(api_keys::id.eq(key_id))
            .set((
                api_keys::key_hash.eq(key_hash),
                api_keys::updated_at.eq(Utc::now()),
            ))
            .get_result(conn)
            .await
            .map_err(Error::Regenerate)
    }

    pub async fn delete(key_id: KeyId, conn: &mut Conn<'_>) -> Result<(), Error> {
        diesel::delete(api_keys::table.find(key_id))
            .execute(conn)
            .await
            .map_err(Error::DeleteKey)
            .and_then(|deleted| match deleted {
                0 => Err(Error::NoKeysDeleted),
                1 => Ok(()),
                n => Err(Error::MultipleKeysDeleted(n)),
            })
    }
}

impl From<&ApiKey> for ResourceEntry {
    fn from(key: &ApiKey) -> Self {
        ResourceEntry::new(key.resource.into(), key.resource_id)
    }
}

impl From<&ApiKey> for Resource {
    fn from(key: &ApiKey) -> Self {
        ResourceEntry::from(key).into()
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = api_keys)]
pub struct NewApiKey {
    user_id: UserId,
    label: String,
    key_hash: KeyHash,
    key_salt: Salt,
    resource: ApiResource,
    resource_id: ResourceId,
}

impl NewApiKey {
    pub async fn create(
        user_id: UserId,
        label: String,
        entry: ResourceEntry,
        conn: &mut WriteConn<'_, '_>,
    ) -> Result<Created, Error> {
        let mut rng = conn.ctx.rng.lock().await;
        let salt = Salt::generate(&mut *rng);
        let secret = Secret::generate(&mut *rng);
        drop(rng);

        let key_hash = KeyHash::from(&salt, &secret);
        let new_api_key = NewApiKey {
            user_id,
            label,
            key_hash,
            key_salt: salt,
            resource: entry.resource_type.into(),
            resource_id: entry.resource_id,
        };

        let api_key: ApiKey = diesel::insert_into(api_keys::table)
            .values(new_api_key)
            .get_result(conn)
            .await
            .map_err(Error::CreateNew)?;

        let secret = BearerSecret::new(api_key.id, &secret);

        Ok(Created { api_key, secret })
    }

    pub async fn regenerate(key_id: KeyId, conn: &mut WriteConn<'_, '_>) -> Result<Created, Error> {
        let existing = ApiKey::find_by_id(key_id, conn).await?;
        let new_secret = {
            let mut rng = conn.ctx.rng.lock().await;
            Secret::generate(&mut *rng)
        };

        let key_hash = KeyHash::from(&existing.key_salt, &new_secret);
        let updated = ApiKey::regenerate(key_id, key_hash, conn).await?;
        let secret = BearerSecret::new(updated.id, &new_secret);

        Ok(Created {
            api_key: updated,
            secret,
        })
    }
}

/// A new `ApiKey` row plus the `BearerSecret` for returning only on creation.
pub struct Created {
    pub api_key: ApiKey,
    pub secret: BearerSecret,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = api_keys)]
pub struct UpdateLabel {
    id: KeyId,
    label: String,
}

impl UpdateLabel {
    pub const fn new(id: KeyId, label: String) -> Self {
        UpdateLabel { id, label }
    }

    pub async fn update(self, conn: &mut Conn<'_>) -> Result<DateTime<Utc>, Error> {
        let updated: ApiKey = diesel::update(api_keys::table.find(self.id))
            .set((self, api_keys::updated_at.eq(Utc::now())))
            .get_result(conn)
            .await
            .map_err(Error::UpdateLabel)?;

        updated.updated_at.ok_or(Error::MissingUpdatedAt)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = api_keys)]
pub struct UpdateScope {
    id: KeyId,
    resource: ApiResource,
    resource_id: ResourceId,
}

impl UpdateScope {
    pub fn new(id: KeyId, entry: ResourceEntry) -> Self {
        UpdateScope {
            id,
            resource: entry.resource_type.into(),
            resource_id: entry.resource_id,
        }
    }

    pub async fn update(self, conn: &mut Conn<'_>) -> Result<DateTime<Utc>, Error> {
        let updated: ApiKey = diesel::update(api_keys::table.find(self.id))
            .set((self, api_keys::updated_at.eq(Utc::now())))
            .get_result(conn)
            .await
            .map_err(Error::UpdateLabel)?;

        updated.updated_at.ok_or(Error::MissingUpdatedAt)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumApiResource"]
#[repr(i32)]
pub enum ApiResource {
    User = 1,
    Org = 2,
    Node = 3,
    Host = 4,
}

impl From<ResourceType> for ApiResource {
    fn from(ty: ResourceType) -> Self {
        match ty {
            ResourceType::User => ApiResource::User,
            ResourceType::Org => ApiResource::Org,
            ResourceType::Node => ApiResource::Node,
            ResourceType::Host => ApiResource::Host,
        }
    }
}

impl From<ApiResource> for ResourceType {
    fn from(api: ApiResource) -> Self {
        match api {
            ApiResource::User => ResourceType::User,
            ApiResource::Org => ResourceType::Org,
            ApiResource::Host => ResourceType::Host,
            ApiResource::Node => ResourceType::Node,
        }
    }
}

impl TryFrom<i32> for ApiResource {
    type Error = Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ApiResource::User),
            2 => Ok(ApiResource::Org),
            3 => Ok(ApiResource::Node),
            4 => Ok(ApiResource::Host),
            n => Err(Error::UnknownApiResource(n)),
        }
    }
}
