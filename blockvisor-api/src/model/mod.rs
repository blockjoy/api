//! This module contains models for interacting with database tables.

pub mod address;
pub use address::{Address, NewAddress};

pub mod api_key;
pub use api_key::ApiKey;

pub mod blockchain;
pub use blockchain::{Blockchain, BlockchainId};

pub mod command;
pub use command::{Command, CommandType};

pub mod host;
pub use host::Host;

pub mod invitation;
pub use invitation::Invitation;

pub mod ip_address;
pub use ip_address::IpAddress;

pub mod node;
pub use node::{Node, NodeType};

pub mod org;
pub use org::Org;

pub mod paginate;
pub use paginate::Paginate;

pub mod rbac;

pub mod region;
pub use region::{Region, RegionId};

#[allow(clippy::wildcard_imports)]
pub mod schema;

pub mod subscription;
pub use subscription::{Subscription, SubscriptionId};

pub mod token;
pub use token::Token;

pub mod user;
pub use user::User;

use derive_more::Display;
use diesel::{define_sql_function, deserialize, expression, pg, serialize, sql_types};

define_sql_function!(fn lower(x: sql_types::Text) -> sql_types::Text);
define_sql_function!(fn string_to_array(version: sql_types::Text, split: sql_types::Text) -> sql_types::Array<diesel::sql_types::Text>);
define_sql_function!(fn text(version: sql_types::Uuid) -> sql_types::Text);
define_sql_function!(fn abbrev(inet: sql_types::Inet) -> sql_types::Text);

#[derive(Debug, Clone, Display, expression::AsExpression, deserialize::FromSqlRow)]
#[diesel(sql_type = sql_types::Text)]
pub struct Url(url::Url);

#[derive(Debug, displaydoc::Display, thiserror::Error)]
enum UrlError {
    /// Failed to parse url `{1}`: {0}
    Parse(url::ParseError, String),
}

impl deserialize::FromSql<sql_types::Text, pg::Pg> for Url {
    fn from_sql(value: pg::PgValue<'_>) -> deserialize::Result<Self> {
        let value: String = deserialize::FromSql::<sql_types::Text, pg::Pg>::from_sql(value)?;
        Ok(Self(value.parse().map_err(|e| UrlError::Parse(e, value))?))
    }
}

impl serialize::ToSql<sql_types::Text, pg::Pg> for Url {
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, pg::Pg>) -> serialize::Result {
        let value = self.to_string();
        <String as serialize::ToSql<sql_types::Text, pg::Pg>>::to_sql(&value, &mut out.reborrow())
    }
}

#[derive(
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    expression::AsExpression,
    deserialize::FromSqlRow,
)]
#[diesel(sql_type = sql_types::Jsonb)]
pub struct Amount {
    pub amount: i64,
    pub currency: Currency,
    pub period: Period,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Usd,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Monthly,
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
enum AmountError {
    /// Failed to parse cost `{1}`: {0}
    Parse(serde_json::Error, serde_json::Value),
}

impl deserialize::FromSql<sql_types::Jsonb, pg::Pg> for Amount {
    fn from_sql(value: pg::PgValue<'_>) -> deserialize::Result<Self> {
        let value: serde_json::Value =
            deserialize::FromSql::<sql_types::Jsonb, pg::Pg>::from_sql(value)?;
        Ok(serde_json::from_value(value.clone()).map_err(|e| AmountError::Parse(e, value))?)
    }
}

impl serialize::ToSql<sql_types::Jsonb, pg::Pg> for Amount {
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, pg::Pg>) -> serialize::Result {
        let value = serde_json::to_value(self)?;
        <serde_json::Value as serialize::ToSql<sql_types::Jsonb, pg::Pg>>::to_sql(
            &value,
            &mut out.reborrow(),
        )
    }
}
