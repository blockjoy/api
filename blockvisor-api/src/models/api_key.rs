use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use thiserror::Error;
use tonic::Status;

use crate::auth::resource::{Resource, ResourceEntry, ResourceId, ResourceType, UserId};
use crate::auth::token::api_key::{BearerSecret, KeyHash, KeyId, Salt, Secret};
use crate::database::{Conn, WriteConn};

use super::schema::api_keys;

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
    pub resource: ResourceType,
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
        ResourceEntry::new(key.resource, key.resource_id)
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
    resource: ResourceType,
    resource_id: ResourceId,
}

impl NewApiKey {
    pub async fn create(
        user_id: UserId,
        label: String,
        entry: ResourceEntry,
        write: &mut WriteConn<'_, '_>,
    ) -> Result<Created, Error> {
        let mut rng = write.ctx.rng.lock().await;
        let salt = Salt::generate(&mut *rng);
        let secret = Secret::generate(&mut *rng);
        drop(rng);

        let key_hash = KeyHash::from(&salt, &secret);
        let new_api_key = NewApiKey {
            user_id,
            label,
            key_hash,
            key_salt: salt,
            resource: entry.resource_type,
            resource_id: entry.resource_id,
        };

        let api_key: ApiKey = diesel::insert_into(api_keys::table)
            .values(new_api_key)
            .get_result(write)
            .await
            .map_err(Error::CreateNew)?;

        let secret = BearerSecret::new(api_key.id, &secret);

        Ok(Created { api_key, secret })
    }

    pub async fn regenerate(
        key_id: KeyId,
        write: &mut WriteConn<'_, '_>,
    ) -> Result<Created, Error> {
        let existing = ApiKey::find_by_id(key_id, write).await?;
        let new_secret = {
            let mut rng = write.ctx.rng.lock().await;
            Secret::generate(&mut *rng)
        };

        let key_hash = KeyHash::from(&existing.key_salt, &new_secret);
        let updated = ApiKey::regenerate(key_id, key_hash, write).await?;
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
    resource: ResourceType,
    resource_id: ResourceId,
}

impl UpdateScope {
    pub const fn new(id: KeyId, entry: ResourceEntry) -> Self {
        UpdateScope {
            id,
            resource: entry.resource_type,
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
