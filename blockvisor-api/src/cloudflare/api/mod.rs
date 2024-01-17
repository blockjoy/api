//! Types reimplemented from <https://crates.io/crates/cloudflare>.

pub mod dns;

use std::collections::HashMap;

use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiSuccess<T> {
    pub result: T,
    pub result_info: Option<serde_json::Value>,
    #[serde(default)]
    pub messages: serde_json::Value,
    #[serde(default)]
    pub errors: Vec<ApiError>,
}

#[derive(Debug, Default, Eq, Serialize, Deserialize)]
pub struct ApiErrors {
    pub errors: Vec<ApiError>,
    #[serde(flatten)]
    pub other: HashMap<String, serde_json::Value>,
}

impl PartialEq for ApiErrors {
    fn eq(&self, other: &Self) -> bool {
        self.errors == other.errors
    }
}

#[derive(Debug, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: u32,
    pub message: String,
    #[serde(flatten)]
    pub other: HashMap<String, serde_json::Value>,
}

impl PartialEq for ApiError {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code && self.message == other.message
    }
}

#[derive(Clone, Debug, Serialize)]
pub enum OrderDirection {
    #[serde(rename = "asc")]
    Ascending,
    #[serde(rename = "desc")]
    Descending,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMatch {
    /// Match all search requirements
    All,
    /// Match at least one search requirement
    Any,
}
