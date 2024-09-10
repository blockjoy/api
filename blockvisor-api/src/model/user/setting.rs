use derive_more::{Deref, From, FromStr, Into};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::UserId;
use crate::database::Conn;
use crate::grpc::Status;
use crate::model::schema::user_settings;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create new user setting: {0}
    Create(diesel::result::Error),
    /// Failed to delete user setting: {0}
    Delete(diesel::result::Error),
    /// Failed to find user settings for user `{0}`: {1}
    ByUser(UserId, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(_) | Delete(_) | ByUser(_, _) => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct UserSettingId(Uuid);

#[derive(Clone, Debug, Display, PartialEq, Eq, DieselNewType, Deref, From, Into)]
pub struct UserSettingKey(String);

#[derive(Debug, Clone, Queryable, AsChangeset)]
#[diesel(table_name = user_settings)]
pub struct UserSetting {
    pub id: UserSettingId,
    pub user_id: UserId,
    pub key: UserSettingKey,
    pub value: Vec<u8>,
}

impl UserSetting {
    pub async fn by_user(user_id: UserId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        user_settings::table
            .filter(user_settings::user_id.eq(user_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByUser(user_id, err))
    }

    pub async fn delete(
        user_id: UserId,
        key: &UserSettingKey,
        conn: &mut Conn<'_>,
    ) -> Result<(), Error> {
        let to_delete = user_settings::table
            .filter(user_settings::user_id.eq(user_id))
            .filter(user_settings::key.eq(key));
        diesel::delete(to_delete)
            .execute(conn)
            .await
            .map_err(Error::Delete)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = user_settings)]
pub struct NewUserSetting<'a> {
    user_id: UserId,
    key: UserSettingKey,
    value: &'a [u8],
}

impl<'a> NewUserSetting<'a> {
    pub const fn new(user_id: UserId, key: String, value: &'a [u8]) -> Self {
        Self {
            user_id,
            key: UserSettingKey(key),
            value,
        }
    }

    pub async fn create_or_update(self, conn: &mut Conn<'_>) -> Result<UserSetting, Error> {
        diesel::insert_into(user_settings::table)
            .values(&self)
            .on_conflict((user_settings::user_id, user_settings::key))
            .do_update()
            .set(user_settings::value.eq(self.value))
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}
