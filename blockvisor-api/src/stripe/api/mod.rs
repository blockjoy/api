//! Types reimplemented from <https://crates.io/crates/cloudflare>.

mod setup_intent;
pub use setup_intent::*;

use reqwest::Method;
use serde::de::DeserializeOwned;

pub trait Endpoint: Send + Sync {
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
