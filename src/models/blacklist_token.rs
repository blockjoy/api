use super::schema::token_blacklist;
use crate::Result;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn can_blacklist_any_token() {
        let tester = crate::TestDb::setup().await;
        let token = "some-fancy-token".to_string();
        let model = BlacklistToken {
            token: token.clone(),
            token_type: TokenType::UserAuth,
        };
        let mut conn = tester.conn().await;
        let blt = model.create(&mut conn).await.unwrap();

        assert_eq!(blt.token, token);
    }
}
