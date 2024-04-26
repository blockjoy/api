//! This module contains the code related to database tables. Each table is represented by one or
//! more models, which are structs that have a field for the columns of the queries that they
//! interact with. There may exist multiple models for a given table, for example models that are
//! used for updating rows often do not contain all of the columns, whereas models that are used
//! for selecting usually do.

pub mod api_key;
pub use api_key::ApiKey;

pub mod blockchain;
pub use blockchain::{Blockchain, BlockchainId};

pub mod command;
pub use command::{Command, CommandId, CommandType};

pub mod host;
pub use host::{Host, HostType};

pub mod invitation;
pub use invitation::{Invitation, InvitationId};

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
use diesel::{deserialize, expression, pg, serialize, sql_function, sql_types};

sql_function!(fn lower(x: sql_types::Text) -> sql_types::Text);
sql_function!(fn string_to_array(version: sql_types::Text, split: sql_types::Text) -> sql_types::Array<diesel::sql_types::Text>);
sql_function!(fn text(version: sql_types::Uuid) -> sql_types::Text);
sql_function!(fn abbrev(inet: sql_types::Inet) -> sql_types::Text);

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
