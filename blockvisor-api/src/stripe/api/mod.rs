//! Types reimplemented from <https://crates.io/crates/cloudflare>.

pub mod account;
pub mod card;
pub mod currency;
pub mod customer;
pub mod event;
pub mod payment_method;
pub mod setup_intent;

use reqwest::Method;
use serde::de::DeserializeOwned;

pub trait StripeEndpoint: Send + Sync {
    type Result: DeserializeOwned;

    /// The HTTP Method used for this endpoint.
    fn method(&self) -> Method;

    /// The relative URL path for this endpoint
    fn path(&self) -> String;

    /// The url-encoded query string associated with this endpoint.
    fn query(&self) -> Option<String> {
        None
    }

    /// The HTTP body associated with this endpoint.
    fn body(&self) -> Option<String> {
        None
    }
}

/// An id or object. By default stripe will return an id for most fields, but if more detail is
/// necessary the `expand` parameter can be provided to ask for the id to be loaded as an object
/// instead. For more details <https://stripe.com/docs/api/expanding_objects>.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum IdOrObject<Id, Object> {
    Id(Id),
    Object(Object),
}

#[derive(Debug, serde::Deserialize)]
pub struct Timestamp(i64);

#[derive(Debug, derive_more::Deref, serde::Serialize, serde::Deserialize)]
pub struct Metadata(std::collections::HashMap<String, String>);

#[derive(Debug, derive_more::Display, serde::Serialize, serde::Deserialize)]
pub struct PaymentMethodId(String);
