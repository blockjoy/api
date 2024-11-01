use derive_more::{Deref, Display, From, FromStr};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::{OrgId, UserId};
use crate::database::Conn;
use crate::grpc::Status;

use super::schema::subscriptions;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to create new subscription: {0}
    CreateNew(diesel::result::Error),
    /// Failed to delete subscription: {0}
    Delete(diesel::result::Error),
    /// Failed to find subscription by external id: {0}
    FindByExternalId(diesel::result::Error),
    /// Failed to find subscription by id: {0}
    FindById(diesel::result::Error),
    /// Failed to find subscription by org: {0}
    FindByOrg(diesel::result::Error),
    /// Failed to find subscription by org and user: {0}
    FindByOrgAndUser(diesel::result::Error),
    /// Failed to find subscription by user: {0}
    FindByUser(diesel::result::Error),
    /// Multiple subscriptions were deleted. This should not happen.
    MultipleDeleted(usize),
    /// No subscription was deleted.
    NoneDeleted,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            CreateNew(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("Already exists.")
            }
            Delete(NotFound)
            | FindById(NotFound)
            | FindByOrg(NotFound)
            | FindByOrgAndUser(NotFound)
            | FindByUser(NotFound)
            | NoneDeleted => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct SubscriptionId(Uuid);

#[derive(Clone, Debug, Queryable)]
pub struct Subscription {
    pub id: SubscriptionId,
    pub org_id: OrgId,
    pub user_id: UserId,
    pub external_id: String,
}

impl Subscription {
    pub async fn by_id(id: SubscriptionId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        subscriptions::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(Error::FindById)
    }

    pub async fn by_org_id(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Option<Self>, Error> {
        let result = subscriptions::table
            .filter(subscriptions::org_id.eq(org_id))
            .get_result(conn)
            .await;

        match result {
            Ok(sub) => Ok(Some(sub)),
            Err(diesel::result::Error::NotFound) => Ok(None),
            Err(err) => Err(Error::FindByOrg(err)),
        }
    }

    pub async fn by_user_id(user_id: UserId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        subscriptions::table
            .filter(subscriptions::user_id.eq(user_id))
            .get_results(conn)
            .await
            .map_err(Error::FindByUser)
    }

    pub async fn by_external_id(external_id: &str, conn: &mut Conn<'_>) -> Result<Self, Error> {
        subscriptions::table
            .filter(subscriptions::external_id.eq(external_id))
            .get_result(conn)
            .await
            .map_err(Error::FindByExternalId)
    }

    pub async fn delete(id: SubscriptionId, conn: &mut Conn<'_>) -> Result<(), Error> {
        let deleted = diesel::delete(subscriptions::table.find(id))
            .execute(conn)
            .await
            .map_err(Error::Delete)?;

        match deleted {
            0 => Err(Error::NoneDeleted),
            1 => Ok(()),
            n => Err(Error::MultipleDeleted(n)),
        }
    }
}

#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = subscriptions)]
pub struct NewSubscription {
    org_id: OrgId,
    user_id: UserId,
    external_id: String,
}

impl NewSubscription {
    pub const fn new(org_id: OrgId, user_id: UserId, external_id: String) -> Self {
        NewSubscription {
            org_id,
            user_id,
            external_id,
        }
    }

    pub async fn create(self, conn: &mut Conn<'_>) -> Result<Subscription, Error> {
        diesel::insert_into(subscriptions::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::CreateNew)
    }
}
