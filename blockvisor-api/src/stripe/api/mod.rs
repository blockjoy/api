//! Types reimplemented from <https://crates.io/crates/cloudflare>.
//!
//! Note that several of the fields in the api definitions are commented out
//! because they often contain nested definition's that we do not use.

pub mod account;
pub mod address;
pub mod card;
pub mod currency;
pub mod customer;
pub mod discount;
pub mod event;
pub mod invoice;
pub mod payment_method;
pub mod plan;
pub mod price;
pub mod setup_intent;
pub mod subscription;

use derive_more::{Deref, Display};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub trait StripeEndpoint: Send + Sync + Sized {
    type Result: DeserializeOwned;

    /// The HTTP Method used for this endpoint.
    fn method(&self) -> Method;

    /// The relative URL path for this endpoint
    fn path(&self) -> String;

    /// The url-encoded query string associated with this endpoint.
    fn query(&self) -> Option<&Self> {
        None
    }

    /// The HTTP body associated with this endpoint.
    fn body(&self) -> Option<&Self> {
        None
    }
}

/// An id or object.
///
/// By default stripe will return an id for most fields, but if more detail is
/// necessary the `expand` parameter can be provided to ask for the id to be
/// loaded as an object instead.
///
/// For more details: <https://stripe.com/docs/api/expanding_objects>.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum IdOrObject<Id, Object> {
    Id(Id),
    Object(Object),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Timestamp(pub i64);

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(value: chrono::DateTime<chrono::Utc>) -> Self {
        Self(value.timestamp())
    }
}

#[derive(Debug, Deref, Serialize, Deserialize)]
pub struct Metadata(std::collections::HashMap<String, String>);

#[derive(Debug, Display, Clone, Serialize, Deserialize)]
pub struct PaymentMethodId(String);

#[derive(Debug, Deserialize)]
pub struct ListResponse<T> {
    pub object: String,
    pub url: String,
    pub has_more: bool,
    pub data: Vec<T>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteResponse {
    pub id: String,
    pub object: String,
    pub deleted: bool,
}
