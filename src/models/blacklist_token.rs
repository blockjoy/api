use super::schema::token_blacklist;
use crate::auth;
use crate::errors::Result;
use diesel::{dsl, prelude::*};
use diesel_async::{AsyncPgConnection, RunQueryDsl};

#[derive(Debug, Clone, Insertable, Queryable)]
#[diesel(table_name = token_blacklist)]
pub struct BlacklistToken {
    pub token: String,
    pub token_type: TokenType,
}

impl BlacklistToken {
    pub async fn create(self, conn: &mut AsyncPgConnection) -> Result<Self> {
        let tkn = diesel::insert_into(token_blacklist::table)
            .values(self)
            .get_result(conn)
            .await?;
        Ok(tkn)
    }

    /// Returns true if token is on the blacklist
    pub async fn is_listed(token: String, conn: &mut AsyncPgConnection) -> Result<bool> {
        let token = token_blacklist::table.filter(token_blacklist::token.eq(token));
        let is_listed = diesel::select(dsl::exists(token)).get_result(conn).await?;

        Ok(is_listed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::TokenType"]
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

impl From<auth::TokenType> for TokenType {
    fn from(value: auth::TokenType) -> Self {
        match value {
            auth::TokenType::HostAuth => Self::HostAuth,
            auth::TokenType::UserAuth => Self::UserAuth,
            auth::TokenType::UserRefresh => Self::UserRefresh,
            auth::TokenType::HostRefresh => Self::HostRefresh,
            auth::TokenType::PwdReset => Self::PwdReset,
            auth::TokenType::RegistrationConfirmation => Self::RegistrationConfirmation,
            auth::TokenType::Invitation => Self::Invitation,
            auth::TokenType::Cookbook => Self::Cookbook,
        }
    }
}
