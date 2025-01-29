use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use thiserror::Error;

use crate::auth::resource::{Resource, ResourceId, ResourceType, UserId};
use crate::auth::token::api_key::{BearerSecret, KeyHash, KeyId, Salt, Secret};
use crate::database::{Conn, WriteConn};
use crate::grpc::{api, common, Status};
use crate::model::sql::Permissions;
use crate::util::NanosUtc;

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
    /// {0} api keys were deleted. This should not happen.
    MultipleKeysDeleted(usize),
    /// No api keys were deleted.
    NoKeysDeleted,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            CreateNew(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("Api key already exists.")
            }
            DeleteKey(NotFound) | FindById(NotFound) | FindByUser(NotFound) | NoKeysDeleted => {
                Status::not_found("Not found.")
            }
            CreateNew(_) | DeleteKey(_) | FindById(_) | FindByUser(_) | MultipleKeysDeleted(_) => {
                Status::internal("Internal error.")
            }
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
    pub permissions: Permissions,
    pub created_at: DateTime<Utc>,
}

impl ApiKey {
    pub async fn by_id(key_id: KeyId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        api_keys::table
            .find(key_id)
            .get_result(conn)
            .await
            .map_err(Error::FindById)
    }

    pub async fn by_user_id(user_id: UserId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        api_keys::table
            .filter(api_keys::user_id.eq(user_id))
            .get_results(conn)
            .await
            .map_err(Error::FindByUser)
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

    pub fn resource(&self) -> Resource {
        Resource::new(self.resource, self.resource_id)
    }
}

impl From<&ApiKey> for Resource {
    fn from(api_key: &ApiKey) -> Resource {
        api_key.resource()
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
    permissions: Permissions,
}

impl NewApiKey {
    pub async fn create(
        user_id: UserId,
        label: String,
        resource: Resource,
        permissions: Permissions,
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
            resource: resource.typ(),
            resource_id: resource.id(),
            permissions,
        };

        let api_key: ApiKey = diesel::insert_into(api_keys::table)
            .values(new_api_key)
            .get_result(write)
            .await
            .map_err(Error::CreateNew)?;

        let secret = BearerSecret::new(api_key.id, &secret);

        Ok(Created { api_key, secret })
    }
}

/// A new `ApiKey` row plus the `BearerSecret` returned once on creation.
pub struct Created {
    pub api_key: ApiKey,
    pub secret: BearerSecret,
}

impl From<ApiKey> for api::ApiKey {
    fn from(api_key: ApiKey) -> Self {
        let resource = Resource::from(&api_key);
        api::ApiKey {
            api_key_id: api_key.id.to_string(),
            label: api_key.label,
            resource: Some(common::Resource::from(resource)),
            permissions: api_key
                .permissions
                .into_iter()
                .map(|perm| perm.to_string())
                .collect(),
            created_at: Some(NanosUtc::from(api_key.created_at).into()),
        }
    }
}
