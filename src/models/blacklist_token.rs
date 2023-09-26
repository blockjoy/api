use diesel::dsl;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::DatabaseError;
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use displaydoc::Display;
use thiserror::Error;
use tonic::Status;

use crate::database::Conn;

use super::schema::{sql_types, token_blacklist};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create new token blacklist: {0}
    Create(diesel::result::Error),
    /// Failed to check if token is blacklisted: {0}
    IsListed(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Debug, Clone, Insertable, Queryable)]
#[diesel(table_name = token_blacklist)]
pub struct BlacklistToken {
    pub token: String,
    pub token_type: TokenType,
}

impl BlacklistToken {
    pub async fn create(self, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::insert_into(token_blacklist::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }

    /// Returns true if token is on the blacklist
    pub async fn is_listed(token: &str, conn: &mut Conn<'_>) -> Result<bool, Error> {
        let filter = token_blacklist::table.filter(token_blacklist::token.eq(token));
        diesel::select(dsl::exists(filter))
            .get_result(conn)
            .await
            .map_err(Error::IsListed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::TokenType"]
pub enum TokenType {
    UserAuth,
    HostAuth,
    UserRefresh,
    HostRefresh,
    PwdReset,
    RegistrationConfirmation,
    Invitation,
    Cookbook,
}

#[cfg(test)]
mod tests {
    use crate::config::Context;

    use super::*;

    #[tokio::test]
    async fn can_blacklist_any_token() {
        let (_ctx, db) = Context::with_mocked().await.unwrap();

        let token = "some-fancy-token".to_string();
        let model = BlacklistToken {
            token: token.clone(),
            token_type: TokenType::UserAuth,
        };
        let mut conn = db.conn().await;
        let blt = model.create(&mut conn).await.unwrap();

        assert_eq!(blt.token, token);
    }
}
