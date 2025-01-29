use diesel::deserialize::{FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::Jsonb;
use diesel::{deserialize, serialize};
use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::auth::rbac::{HostAdminPerm, NodeAdminPerm};
use crate::auth::AuthZ;
use crate::grpc::{common, Status};
use crate::model::{Host, Node};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Billing amount provided without an `amount` field.
    NoAmount,
    /// Failed to parse Amount `{0}`: {1}
    ParseAmount(serde_json::Value, serde_json::Error),
    /// Unknown currency.
    UnknownCurrency,
    /// Unknown period.
    UnknownPeriod,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            NoAmount => Status::invalid_argument("cost"),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, AsExpression, FromSqlRow)]
#[diesel(sql_type = Jsonb)]
pub struct Amount {
    pub amount: i64,
    pub currency: Currency,
    pub period: Period,
}

impl TryFrom<common::BillingAmount> for Amount {
    type Error = Error;

    fn try_from(billing: common::BillingAmount) -> Result<Self, Self::Error> {
        let amount = billing.amount.ok_or(Error::NoAmount)?;
        Ok(Amount {
            amount: amount.amount_minor_units,
            currency: amount.currency().try_into()?,
            period: billing.period().try_into()?,
        })
    }
}

impl FromSql<Jsonb, Pg> for Amount {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let value: serde_json::Value = FromSql::<Jsonb, Pg>::from_sql(value)?;
        Amount::deserialize(&value).map_err(|err| Error::ParseAmount(value, err).into())
    }
}

impl ToSql<Jsonb, Pg> for Amount {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let value = serde_json::to_value(self)?;
        <serde_json::Value as ToSql<Jsonb, Pg>>::to_sql(&value, &mut out.reborrow())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Currency {
    Usd,
}

impl From<Currency> for common::Currency {
    fn from(value: Currency) -> Self {
        match value {
            Currency::Usd => common::Currency::Usd,
        }
    }
}

impl TryFrom<common::Currency> for Currency {
    type Error = Error;

    fn try_from(value: common::Currency) -> Result<Self, Self::Error> {
        match value {
            common::Currency::Usd => Ok(Currency::Usd),
            common::Currency::Unspecified => Err(Error::UnknownPeriod),
        }
    }
}

impl common::Currency {
    pub const fn from_stripe(value: crate::stripe::api::currency::Currency) -> Option<Self> {
        use crate::stripe::api::currency::Currency::*;
        match value {
            USD => Some(common::Currency::Usd),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Period {
    Monthly,
}

impl From<Period> for common::Period {
    fn from(value: Period) -> Self {
        match value {
            Period::Monthly => common::Period::Monthly,
        }
    }
}

impl TryFrom<common::Period> for Period {
    type Error = Error;

    fn try_from(value: common::Period) -> Result<Self, Self::Error> {
        match value {
            common::Period::Monthly => Ok(Period::Monthly),
            common::Period::Unspecified => Err(Error::UnknownPeriod),
        }
    }
}

impl common::BillingAmount {
    pub fn from_host(host: &Host, authz: &AuthZ) -> Option<Self> {
        if !authz.has_perm(HostAdminPerm::ViewCost) {
            return None;
        }

        let cost = host.cost?;
        Some(common::BillingAmount {
            amount: Some(common::Amount {
                currency: common::Currency::from(cost.currency).into(),
                amount_minor_units: cost.amount,
            }),
            period: common::Period::from(cost.period).into(),
        })
    }

    pub fn from_node(node: &Node, authz: &AuthZ) -> Option<Self> {
        if !authz.has_perm(NodeAdminPerm::ViewCost) {
            return None;
        }

        let cost = node.cost?;
        Some(common::BillingAmount {
            amount: Some(common::Amount {
                currency: common::Currency::from(cost.currency).into(),
                amount_minor_units: cost.amount,
            }),
            period: common::Period::from(cost.period).into(),
        })
    }
}
