//!
//! curl -X POST "https://api.cloudflare.com/client/v4/zones/89560cdd783e35f7a9d718755ea9c656/dns_records" \
//!      -H "Authorization: Bearer 9QjEiXC4B26tgshHZjuZ57kJcjaChSSsDfzUvfYQ" \
//!      -H "Content-Type: application/json" \
//!      --data '{"type":"A","name":"meow.n0des.xyz","content":"127.0.0.1","ttl":3600,"priority":10,"proxied":false,"comment":"Chain node record", "tags": ["owner": <guid>]}'
//!

use std::string::ToString;
use std::sync::Arc;

use anyhow::anyhow;
use axum::http;
use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::cloudflare::Config;

pub type DnsResult<T> = Result<T, DnsError>;

#[derive(Debug, Display, Error)]
pub enum DnsError {
    /// Error requesting DNS entry: {0}
    Http(#[from] reqwest::Error),
    /// Unknown DNS error: {0}
    Unknown(anyhow::Error),
}

#[derive(Deserialize, Debug)]
struct CloudflareDnsResult {
    pub id: String,
}

#[derive(Deserialize, Debug)]
struct CloudflareDnsResponse {
    pub result: Option<CloudflareDnsResult>,
}

#[derive(Serialize, Debug)]
pub struct CloudflarePayload {
    pub r#type: String,
    pub name: String,
    pub content: String,
    pub ttl: i64,
    pub priority: i32,
    pub proxied: bool,
}

#[derive(Clone)]
pub struct CloudflareApi {
    pub config: Arc<Config>,
}

impl CloudflareApi {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    pub fn payload(&self, node_name: &str, origin_ip: String) -> CloudflarePayload {
        CloudflarePayload {
            r#type: "A".to_string(),
            name: format!("{node_name}.{}", self.config.dns.base),
            content: origin_ip,
            ttl: self.config.dns.ttl,
            priority: 10,
            proxied: false,
        }
    }

    pub async fn get_node_dns(&self, node_name: &str, origin_ip: String) -> DnsResult<String> {
        let payload = self.payload(node_name, origin_ip);
        let endpoint = format!("zones/{}/dns_records", self.config.api.zone_id);

        match self.post(payload, endpoint).await {
            Ok(response) => {
                tracing::debug!("received response: {response:?}");

                let id = response
                    .result
                    .ok_or_else(|| DnsError::Unknown(anyhow!("Response result is not parsable")))?
                    .id;
                tracing::debug!("Created DNS entry for node name '{node_name}': {}", id);

                Ok(id)
            }
            Err(e) => {
                tracing::error!("Couldn't create DNS entry for node '{node_name}': {e}",);
                Err(DnsError::Unknown(anyhow!(e)))
            }
        }
    }

    pub async fn remove_node_dns(&self, id: String) -> DnsResult<()> {
        let endpoint = format!("zones/{}/dns_records/{}", self.config.api.zone_id, id);

        match self.delete(endpoint).await {
            Ok(_) => Ok(()),
            Err(e) => Err(DnsError::Unknown(anyhow!("Couldn't delete DNS entry: {e}"))),
        }
    }

    async fn post(
        &self,
        payload: CloudflarePayload,
        endpoint: String,
    ) -> DnsResult<CloudflareDnsResponse> {
        let url = format!("{}/{}", self.config.api.base_url, endpoint);
        let client = reqwest::Client::new();

        tracing::debug!("Sending payload to cloudflare: {payload:?}");

        let res = client
            .post(url)
            .bearer_auth(self.config.api.token.as_str())
            .json(&payload)
            .send()
            .await?
            .json::<CloudflareDnsResponse>()
            .await?;

        Ok(res)
    }

    async fn delete(&self, endpoint: String) -> DnsResult<http::status::StatusCode> {
        let url = format!("{}/{}", self.config.api.base_url, endpoint);
        let client = reqwest::Client::new();
        let res = client
            .delete(url)
            .bearer_auth(self.config.api.token.as_str())
            .send()
            .await?;

        Ok(res.status())
    }
}
