pub mod amount;
pub use amount::{Amount, Currency, Period};

use std::fmt;
use std::str::FromStr;

use derive_more::{Deref, Display, From, Into, IntoIterator};
use diesel::deserialize::{FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::{Array, Inet, Jsonb, Nullable, SingleValue, Text};
use diesel::{define_sql_function, deserialize, serialize};
use displaydoc::Display as DisplayDoc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::auth::claims::Granted;
use crate::auth::rbac::Perm;
use crate::grpc::{common, Status};
use crate::model::protocol::VersionMetadata;
use crate::util::LOWER_KEBAB_CASE;

define_sql_function!(fn coalesce(x: Nullable<Text>, y: Text) -> Text);
define_sql_function!(fn greatest<T: SingleValue>(x: T, y: T) -> T);
define_sql_function!(fn lower(x: Text) -> Text);
define_sql_function!(fn string_to_array(text: Text, split: Text) -> Array<Text>);
define_sql_function!(fn text<T: SingleValue>(x: T) -> Text);

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to parse IP `{0}`: {1}
    ParseIp(String, ipnetwork::IpNetworkError),
    /// Failed to parse Perm `{0}`: {1}
    ParsePerm(String, String),
    /// Failed to parse Url `{0}`: {1}
    ParseUrl(String, url::ParseError),
    /// Failed to parse Version `{0}`: {1}
    ParseVersion(String, semver::Error),
    /// Failed to parse ProtocolVersionMetadata `{0}`: {1}
    ParseVersionMetadata(serde_json::Value, serde_json::Error),
    /// Tag is not lower-kebab-case: {0}
    TagChars(String),
    /// Tag must be at least 3 characters: {0}
    TagLen(String),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            TagChars(_) | TagLen(_) => Status::invalid_argument("tag"),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Debug, Display, AsExpression, FromSqlRow)]
#[diesel(sql_type = Text)]
pub struct Url(url::Url);

impl FromStr for Url {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.parse()
            .map(Self)
            .map_err(|err| Error::ParseUrl(text.into(), err))
    }
}

impl FromSql<Text, Pg> for Url {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let text = std::str::from_utf8(value.as_bytes())?;
        text.parse().map(Self).map_err(Into::into)
    }
}

impl ToSql<Text, Pg> for Url {
    fn to_sql<'a>(&'a self, out: &mut Output<'a, '_, Pg>) -> serialize::Result {
        <String as ToSql<Text, Pg>>::to_sql(&self.to_string(), &mut out.reborrow())
    }
}

#[derive(Clone, Copy, Debug, Deref, PartialEq, Eq, AsExpression, FromSqlRow, From, Into)]
#[diesel(sql_type = Inet)]
pub struct IpNetwork(ipnetwork::IpNetwork);

impl FromStr for IpNetwork {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.parse()
            .map(Self)
            .map_err(|err| Error::ParseIp(text.into(), err))
    }
}

impl fmt::Display for IpNetwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ip())
    }
}

impl FromSql<Inet, Pg> for IpNetwork {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let ip = <ipnetwork::IpNetwork as FromSql<Inet, Pg>>::from_sql(value)?;
        Ok(IpNetwork(ip))
    }
}

impl ToSql<Inet, Pg> for IpNetwork {
    fn to_sql<'a>(&'a self, out: &mut Output<'a, '_, Pg>) -> serialize::Result {
        <ipnetwork::IpNetwork as ToSql<Inet, Pg>>::to_sql(&self.0, &mut out.reborrow())
    }
}

#[derive(
    Clone, Debug, Deref, Display, PartialEq, Eq, PartialOrd, Ord, AsExpression, FromSqlRow, From,
)]
#[diesel(sql_type = Text)]
pub struct Version(semver::Version);

impl FromStr for Version {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.to_lowercase()
            .parse()
            .map(Self)
            .map_err(|err| Error::ParseVersion(text.into(), err))
    }
}

impl FromSql<Text, Pg> for Version {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let text = std::str::from_utf8(value.as_bytes())?;
        text.parse().map(Self).map_err(Into::into)
    }
}

impl ToSql<Text, Pg> for Version {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <String as ToSql<Text, Pg>>::to_sql(&self.0.to_string(), &mut out.reborrow())
    }
}

#[derive(Clone, Debug, Display, PartialEq, Eq)]
pub struct Tag(String);

impl Tag {
    pub fn new(tag: String) -> Result<Self, Error> {
        if tag.len() < 3 {
            Err(Error::TagLen(tag))
        } else if !tag.chars().all(|c| LOWER_KEBAB_CASE.contains(c)) {
            Err(Error::TagChars(tag))
        } else {
            Ok(Tag(tag))
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, AsExpression, FromSqlRow, From, IntoIterator)]
#[diesel(sql_type = Array<Nullable<Text>>)]
pub struct Tags(Vec<Tag>);

impl FromSql<Array<Nullable<Text>>, Pg> for Tags {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let tags = <Vec<Option<String>> as FromSql<Array<Nullable<Text>>, Pg>>::from_sql(value)?;
        tags.into_iter()
            .filter_map(|opt| opt.map(Tag::new))
            .collect::<Result<Vec<Tag>, Error>>()
            .map(Tags)
            .map_err(Into::into)
    }
}

impl ToSql<Array<Nullable<Text>>, Pg> for Tags {
    fn to_sql(&self, out: &mut Output<'_, '_, Pg>) -> serialize::Result {
        let tags: Vec<Option<&str>> = self.0.iter().map(|tag| Some(tag.0.as_str())).collect();
        <Vec<Option<&str>> as ToSql<Array<Nullable<Text>>, Pg>>::to_sql(&tags, &mut out.reborrow())
    }
}

impl common::UpdateTags {
    pub fn into_update(self, current_tags: Tags) -> Result<Option<Tags>, Error> {
        match self.update {
            Some(common::update_tags::Update::OverwriteTags(tags)) => {
                let new_tags = tags
                    .tags
                    .into_iter()
                    .map(|tag| Tag::new(tag.name))
                    .collect::<Result<Vec<Tag>, Error>>()?;
                Ok(Some(Tags(new_tags)))
            }
            Some(common::update_tags::Update::AddTag(tag)) => {
                let new_tag = Tag::new(tag.name)?;
                let new_tags = current_tags.0.into_iter().chain([new_tag]).collect();
                Ok(Some(Tags(new_tags)))
            }
            None => Ok(None),
        }
    }
}

impl From<Tags> for common::Tags {
    fn from(tags: Tags) -> Self {
        common::Tags {
            tags: tags
                .into_iter()
                .map(|tag| common::Tag { name: tag.0 })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, AsExpression, FromSqlRow, IntoIterator)]
#[diesel(sql_type = Array<Nullable<Text>>)]
pub struct Permissions(Vec<Perm>);

impl Permissions {
    pub fn from(granted: Granted) -> Self {
        Permissions(granted.into_iter().collect())
    }
}

impl FromSql<Array<Nullable<Text>>, Pg> for Permissions {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let perms = <Vec<Option<String>> as FromSql<Array<Nullable<Text>>, Pg>>::from_sql(value)?;
        perms
            .into_iter()
            .filter_map(|opt| {
                opt.map(|perm| perm.parse().map_err(|err| Error::ParsePerm(perm, err)))
            })
            .collect::<Result<Vec<Perm>, Error>>()
            .map(Permissions)
            .map_err(Into::into)
    }
}

impl ToSql<Array<Nullable<Text>>, Pg> for Permissions {
    fn to_sql(&self, out: &mut Output<'_, '_, Pg>) -> serialize::Result {
        let perms: Vec<Option<String>> = self.0.iter().map(|perm| Some(perm.to_string())).collect();
        <Vec<Option<String>> as ToSql<Array<Nullable<Text>>, Pg>>::to_sql(
            &perms,
            &mut out.reborrow(),
        )
    }
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    AsExpression,
    FromSqlRow,
    From,
    IntoIterator,
)]
#[diesel(sql_type = Jsonb)]
pub struct ProtocolVersionMetadata(Vec<VersionMetadata>);

impl FromSql<Jsonb, Pg> for ProtocolVersionMetadata {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let value: serde_json::Value = FromSql::<Jsonb, Pg>::from_sql(value)?;
        ProtocolVersionMetadata::deserialize(&value)
            .map_err(|err| Error::ParseVersionMetadata(value, err).into())
    }
}

impl ToSql<Jsonb, Pg> for ProtocolVersionMetadata {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let value = serde_json::to_value(self)?;
        <serde_json::Value as ToSql<Jsonb, Pg>>::to_sql(&value, &mut out.reborrow())
    }
}
