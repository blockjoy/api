use std::fmt;

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display;
use rand::distributions::Alphanumeric;
use rand::Rng;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::{OrgId, ResourceId, ResourceType, UserId};
use crate::database::Conn;
use crate::grpc::Status;

use super::schema::{sql_types, tokens};

const HOST_PROVISION_LEN: usize = 12;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to delete host provision token for user `{0}` and org `{1}`: {2}
    DeleteHostProvision(UserId, OrgId, diesel::result::Error),
    /// Failed to find host provision token: {0}
    HostProvisionByToken(diesel::result::Error),
    /// Failed to get host provision token for user `{0}` and org `{1}`: {2}
    HostProvisionByUser(UserId, OrgId, diesel::result::Error),
    /// Failed to create new host provision token: {0}
    NewHostProvision(diesel::result::Error),
    /// Failed to reset host provision token for user `{0}` and org `{1}`: {2}
    ResetHostProvision(UserId, OrgId, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            DeleteHostProvision(_, _, NotFound)
            | HostProvisionByToken(NotFound)
            | HostProvisionByUser(_, _, NotFound)
            | ResetHostProvision(_, _, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumTokenType"]
pub enum TokenType {
    HostProvision,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, DieselNewType)]
pub struct TokenId(Uuid);

#[derive(Debug, Queryable)]
#[diesel(table_name = tokens)]
pub struct Token {
    pub id: TokenId,
    pub token_type: TokenType,
    pub token: TokenValue,
    pub created_by_resource: ResourceType,
    pub created_by: ResourceId,
    pub org_id: Option<OrgId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Token {
    pub async fn new_host_provision(
        user_id: UserId,
        org_id: OrgId,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let token = TokenValue::new(HOST_PROVISION_LEN);

        diesel::insert_into(tokens::table)
            .values((
                tokens::token_type.eq(TokenType::HostProvision),
                tokens::token.eq(token),
                tokens::created_by_resource.eq(ResourceType::User),
                tokens::created_by.eq(user_id),
                tokens::org_id.eq(org_id),
            ))
            .get_result(conn)
            .await
            .map_err(Error::NewHostProvision)
    }

    pub async fn host_provision_by_user(
        user_id: UserId,
        org_id: OrgId,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        tokens::table
            .filter(tokens::token_type.eq(TokenType::HostProvision))
            .filter(tokens::created_by_resource.eq(ResourceType::User))
            .filter(tokens::created_by.eq(user_id))
            .filter(tokens::org_id.eq(org_id))
            .get_result(conn)
            .await
            .map_err(|err| Error::HostProvisionByUser(user_id, org_id, err))
    }

    pub async fn host_provision_by_token(token: &str, conn: &mut Conn<'_>) -> Result<Self, Error> {
        tokens::table
            .filter(tokens::token_type.eq(TokenType::HostProvision))
            .filter(tokens::token.eq(token))
            .get_result(conn)
            .await
            .map_err(Error::HostProvisionByToken)
    }

    pub async fn reset_host_provision(
        user_id: UserId,
        org_id: OrgId,
        conn: &mut Conn<'_>,
    ) -> Result<TokenValue, Error> {
        let token = TokenValue::new(HOST_PROVISION_LEN);
        let filter = tokens::table
            .filter(tokens::token_type.eq(TokenType::HostProvision))
            .filter(tokens::created_by_resource.eq(ResourceType::User))
            .filter(tokens::created_by.eq(user_id))
            .filter(tokens::org_id.eq(org_id));

        diesel::update(filter)
            .set((tokens::token.eq(&token), tokens::updated_at.eq(Utc::now())))
            .execute(conn)
            .await
            .map_err(|err| Error::ResetHostProvision(user_id, org_id, err))?;

        Ok(token)
    }

    pub async fn delete_host_provision(
        user_id: UserId,
        org_id: OrgId,
        conn: &mut Conn<'_>,
    ) -> Result<(), Error> {
        let filter = tokens::table
            .filter(tokens::token_type.eq(TokenType::HostProvision))
            .filter(tokens::created_by_resource.eq(ResourceType::User))
            .filter(tokens::created_by.eq(user_id))
            .filter(tokens::org_id.eq(org_id));

        diesel::delete(filter)
            .execute(conn)
            .await
            .map_err(|err| Error::DeleteHostProvision(user_id, org_id, err))?;

        Ok(())
    }

    pub fn user(&self) -> Option<UserId> {
        if self.created_by_resource == ResourceType::User {
            Some((*self.created_by).into())
        } else {
            None
        }
    }
}

#[derive(PartialEq, Eq, DieselNewType)]
pub struct TokenValue(String);

impl TokenValue {
    fn new(len: usize) -> Self {
        let text = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(len)
            .map(char::from)
            .collect();
        Self(text)
    }

    pub fn take(self) -> String {
        self.0
    }
}

impl fmt::Debug for TokenValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<Redacted TokenValue>")
    }
}
