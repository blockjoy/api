//! Types reimplemented from <https://crates.io/crates/cloudflare>.
//! Note that several of the fields in the api definitions are commented out. We do this because
//! they often contain nested definition's that we do not use, and we don't want to pay the
//! maintenance / binary size / compilation time cost of having them commented in. If you do need a
//! field that is commented out, feel free to comment it in.

pub mod account;
pub mod card;
pub mod currency;
pub mod customer;
pub mod event;
pub mod payment_method;
pub mod setup_intent;
pub mod subscription;

use reqwest::Method;
use serde::de::DeserializeOwned;

pub trait StripeEndpoint: Send + Sync + Sized {
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
    fn body(&self) -> Option<&Self> {
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
pub struct Timestamp(pub i64);

#[derive(Debug, derive_more::Deref, serde::Serialize, serde::Deserialize)]
pub struct Metadata(std::collections::HashMap<String, String>);

#[derive(Debug, derive_more::Display, serde::Serialize, serde::Deserialize)]
pub struct PaymentMethodId(String);

/// The resource representing a Stripe "Address".
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Address {
    /// City, district, suburb, town, or village.
    pub city: Option<String>,
    /// Two-letter country code ([ISO 3166-1 alpha-2]
    /// (https://en.wikipedia.org/wiki/ISO_3166-1_alpha-2)).
    pub country: Option<String>,
    /// Address line 1 (e.g., street, PO Box, or company name).
    pub line1: Option<String>,
    /// Address line 2 (e.g., apartment, suite, unit, or building).
    pub line2: Option<String>,
    /// ZIP or postal code.
    pub postal_code: Option<String>,
    /// State, county, province, or region.
    pub state: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ListResponse<T> {
    pub object: String,
    pub url: String,
    pub has_more: bool,
    pub data: Vec<T>,
}
