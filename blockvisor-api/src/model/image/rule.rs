use cidr::IpCidr;
use derive_more::{Deref, Display, From, IntoIterator};
use diesel::deserialize::{FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::sql_types::Jsonb;
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel::serialize::{Output, ToSql};
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tonic::Status;
use uuid::Uuid;

use crate::database::Conn;
use crate::grpc::common;
use crate::model::schema::{image_rules, sql_types};

use super::ImageId;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to bulk create firewall rules: {0}
    BulkCreate(diesel::result::Error),
    /// Failed to get image firewall rule for id {0}: {1}
    ById(ImageRuleId, diesel::result::Error),
    /// Failed to get image firewall rules for image id {0}: {1}
    ByImageId(ImageId, diesel::result::Error),
    /// Failed to parse IP as CIDR: {0}
    ParseIpCidr(cidr::errors::NetworkParseError),
    /// Failed to parse port as u16: {0}
    ParsePort(std::num::TryFromIntError),
    /// Unknown FirewallAction.
    UnknownAction,
    /// Unknown FirewallDirection.
    UnknownDirection,
    /// Unknown FirewallProtocol.
    UnknownProtocol,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ById(_, NotFound) => Status::not_found("Not found."),
            UnknownAction => Status::invalid_argument("action"),
            UnknownDirection => Status::invalid_argument("direction"),
            UnknownProtocol => Status::invalid_argument("protocol"),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From)]
pub struct ImageRuleId(Uuid);

#[derive(Clone, Debug, Queryable)]
#[diesel(table_name = image_rules)]
pub struct ImageRule {
    pub id: ImageRuleId,
    pub image_id: ImageId,
    pub key: FirewallRuleKey,
    pub description: Option<String>,
    pub protocol: FirewallProtocol,
    pub direction: FirewallDirection,
    pub action: FirewallAction,
    pub ips: Option<IpNames>,
    pub ports: Option<PortNames>,
}

impl ImageRule {
    pub async fn by_id(id: ImageRuleId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        image_rules::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::ById(id, err))
    }

    pub async fn by_image_id(image_id: ImageId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        image_rules::table
            .filter(image_rules::image_id.eq(image_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByImageId(image_id, err))
    }
}

impl From<ImageRule> for FirewallRule {
    fn from(rule: ImageRule) -> Self {
        FirewallRule {
            key: rule.key,
            description: rule.description,
            protocol: rule.protocol,
            direction: rule.direction,
            action: rule.action,
            ips: rule.ips,
            ports: rule.ports,
        }
    }
}

impl From<ImageRule> for common::FirewallRule {
    fn from(rule: ImageRule) -> Self {
        common::FirewallRule {
            key: rule.key.0,
            description: rule.description,
            protocol: common::FirewallProtocol::from(rule.protocol).into(),
            direction: common::FirewallDirection::from(rule.direction).into(),
            action: common::FirewallAction::from(rule.action).into(),
            ips: rule
                .ips
                .map(|ips| ips.0.into_iter().map(Into::into).collect())
                .unwrap_or_default(),
            ports: rule
                .ports
                .map(|ports| ports.0.into_iter().map(Into::into).collect())
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = image_rules)]
pub struct NewImageRule {
    pub image_id: ImageId,
    pub key: FirewallRuleKey,
    pub description: Option<String>,
    pub protocol: FirewallProtocol,
    pub direction: FirewallDirection,
    pub action: FirewallAction,
    pub ips: Option<IpNames>,
    pub ports: Option<PortNames>,
}

impl NewImageRule {
    pub fn from_existing(image_id: ImageId, rule: ImageRule) -> Self {
        NewImageRule {
            image_id,
            key: rule.key,
            description: rule.description,
            protocol: rule.protocol,
            direction: rule.direction,
            action: rule.action,
            ips: rule.ips,
            ports: rule.ports,
        }
    }

    pub fn from_api(image_id: ImageId, rule: common::FirewallRule) -> Result<Self, Error> {
        let rule = FirewallRule::try_from(rule)?;

        Ok(NewImageRule {
            image_id,
            key: rule.key,
            description: rule.description,
            protocol: rule.protocol,
            direction: rule.direction,
            action: rule.action,
            ips: rule.ips,
            ports: rule.ports,
        })
    }

    pub async fn bulk_create(
        rules: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<ImageRule>, Error> {
        diesel::insert_into(image_rules::table)
            .values(rules)
            .get_results(conn)
            .await
            .map_err(Error::BulkCreate)
    }
}

#[derive(Clone, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From)]
pub struct FirewallRuleKey(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirewallRule {
    pub key: FirewallRuleKey,
    pub description: Option<String>,
    pub protocol: FirewallProtocol,
    pub direction: FirewallDirection,
    pub action: FirewallAction,
    pub ips: Option<IpNames>,
    pub ports: Option<PortNames>,
}

impl From<FirewallRule> for common::FirewallRule {
    fn from(rule: FirewallRule) -> Self {
        common::FirewallRule {
            key: rule.key.0,
            description: rule.description,
            protocol: common::FirewallProtocol::from(rule.protocol).into(),
            direction: common::FirewallDirection::from(rule.direction).into(),
            action: common::FirewallAction::from(rule.action).into(),
            ips: rule
                .ips
                .map(|ips| ips.0.into_iter().map(Into::into).collect())
                .unwrap_or_default(),
            ports: rule
                .ports
                .map(|ports| ports.0.into_iter().map(Into::into).collect())
                .unwrap_or_default(),
        }
    }
}

impl TryFrom<common::FirewallRule> for FirewallRule {
    type Error = Error;

    fn try_from(rule: common::FirewallRule) -> Result<Self, Self::Error> {
        let protocol = rule.protocol().try_into()?;
        let direction = rule.direction().try_into()?;
        let action = rule.action().try_into()?;

        let ips = rule
            .ips
            .into_iter()
            .map(IpName::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let ports = rule
            .ports
            .into_iter()
            .map(PortName::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(FirewallRule {
            key: FirewallRuleKey(rule.key),
            description: rule.description,
            protocol,
            direction,
            action,
            ips: (!ips.is_empty()).then_some(IpNames(ips)),
            ports: (!ports.is_empty()).then_some(PortNames(ports)),
        })
    }
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    AsExpression,
    From,
    FromSqlRow,
    IntoIterator,
    Serialize,
    Deserialize,
)]
#[diesel(sql_type = Jsonb)]
pub struct IpNames(pub Vec<IpName>);

impl FromSql<Jsonb, Pg> for IpNames {
    fn from_sql(value: PgValue<'_>) -> diesel::deserialize::Result<Self> {
        serde_json::from_value(FromSql::<Jsonb, Pg>::from_sql(value)?).map_err(Into::into)
    }
}

impl ToSql<Jsonb, Pg> for IpNames {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        let json = serde_json::to_value(self).unwrap();
        <serde_json::Value as ToSql<Jsonb, Pg>>::to_sql(&json, &mut out.reborrow())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpName {
    pub ip: IpCidr,
    pub name: Option<String>,
}

impl From<IpName> for common::IpName {
    fn from(ip: IpName) -> Self {
        Self {
            ip: ip.ip.to_string(),
            name: ip.name,
        }
    }
}

impl TryFrom<common::IpName> for IpName {
    type Error = Error;

    fn try_from(ip: common::IpName) -> Result<Self, Self::Error> {
        Ok(Self {
            ip: ip.ip.parse().map_err(Error::ParseIpCidr)?,
            name: ip.name,
        })
    }
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    AsExpression,
    From,
    FromSqlRow,
    IntoIterator,
    Serialize,
    Deserialize,
)]
#[diesel(sql_type = Jsonb)]
pub struct PortNames(pub Vec<PortName>);

impl FromSql<Jsonb, Pg> for PortNames {
    fn from_sql(value: PgValue<'_>) -> diesel::deserialize::Result<Self> {
        serde_json::from_value(FromSql::<Jsonb, Pg>::from_sql(value)?).map_err(Into::into)
    }
}

impl ToSql<Jsonb, Pg> for PortNames {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        let json = serde_json::to_value(self).unwrap();
        <serde_json::Value as ToSql<Jsonb, Pg>>::to_sql(&json, &mut out.reborrow())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortName {
    pub port: u16,
    pub name: Option<String>,
}

impl From<PortName> for common::PortName {
    fn from(port: PortName) -> Self {
        Self {
            port: u32::from(port.port),
            name: port.name,
        }
    }
}

impl TryFrom<common::PortName> for PortName {
    type Error = Error;

    fn try_from(ip: common::PortName) -> Result<Self, Self::Error> {
        Ok(Self {
            port: u16::try_from(ip.port).map_err(Error::ParsePort)?,
            name: ip.name,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumFirewallProtocol"]
pub enum FirewallProtocol {
    Tcp,
    Udp,
    Both,
}

impl From<FirewallProtocol> for common::FirewallProtocol {
    fn from(action: FirewallProtocol) -> Self {
        match action {
            FirewallProtocol::Tcp => common::FirewallProtocol::Tcp,
            FirewallProtocol::Udp => common::FirewallProtocol::Udp,
            FirewallProtocol::Both => common::FirewallProtocol::Both,
        }
    }
}

impl TryFrom<common::FirewallProtocol> for FirewallProtocol {
    type Error = Error;

    fn try_from(action: common::FirewallProtocol) -> Result<Self, Self::Error> {
        match action {
            common::FirewallProtocol::Unspecified => Err(Error::UnknownProtocol),
            common::FirewallProtocol::Tcp => Ok(FirewallProtocol::Tcp),
            common::FirewallProtocol::Udp => Ok(FirewallProtocol::Udp),
            common::FirewallProtocol::Both => Ok(FirewallProtocol::Both),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumFirewallDirection"]
pub enum FirewallDirection {
    Inbound,
    Outbound,
}

impl From<FirewallDirection> for common::FirewallDirection {
    fn from(action: FirewallDirection) -> Self {
        match action {
            FirewallDirection::Inbound => common::FirewallDirection::Inbound,
            FirewallDirection::Outbound => common::FirewallDirection::Outbound,
        }
    }
}

impl TryFrom<common::FirewallDirection> for FirewallDirection {
    type Error = Error;

    fn try_from(action: common::FirewallDirection) -> Result<Self, Self::Error> {
        match action {
            common::FirewallDirection::Unspecified => Err(Error::UnknownDirection),
            common::FirewallDirection::Inbound => Ok(FirewallDirection::Inbound),
            common::FirewallDirection::Outbound => Ok(FirewallDirection::Outbound),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumFirewallAction"]
pub enum FirewallAction {
    Allow,
    Drop,
    Reject,
}

impl From<FirewallAction> for common::FirewallAction {
    fn from(action: FirewallAction) -> Self {
        match action {
            FirewallAction::Allow => common::FirewallAction::Allow,
            FirewallAction::Drop => common::FirewallAction::Drop,
            FirewallAction::Reject => common::FirewallAction::Reject,
        }
    }
}

impl TryFrom<common::FirewallAction> for FirewallAction {
    type Error = Error;

    fn try_from(action: common::FirewallAction) -> Result<Self, Self::Error> {
        match action {
            common::FirewallAction::Unspecified => Err(Error::UnknownAction),
            common::FirewallAction::Allow => Ok(FirewallAction::Allow),
            common::FirewallAction::Drop => Ok(FirewallAction::Drop),
            common::FirewallAction::Reject => Ok(FirewallAction::Reject),
        }
    }
}
