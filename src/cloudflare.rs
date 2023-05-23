//!
//! curl -X POST "https://api.cloudflare.com/client/v4/zones/89560cdd783e35f7a9d718755ea9c656/dns_records" \
//!      -H "Authorization: Bearer 9QjEiXC4B26tgshHZjuZ57kJcjaChSSsDfzUvfYQ" \
//!      -H "Content-Type: application/json" \
//!      --data '{"type":"A","name":"meow.n0des.xyz","content":"127.0.0.1","ttl":3600,"priority":10,"proxied":false,"comment":"Chain node record", "tags": ["owner": <guid>]}'
//!

use crate::auth::key_provider::KeyProvider;
use anyhow::anyhow;
use axum::http;
use serde::{Deserialize, Serialize};
use std::string::ToString;

pub type DnsResult<T> = Result<T, DnsError>;

const CF_DNS_BASE: &str = "CF_DNS_BASE";
const CF_TTL: &str = "CF_TTL";
const CF_ZONE: &str = "CF_ZONE";
const CF_BASE_URL: &str = "CF_BASE_URL";
const CF_TOKEN: &str = "CF_TOKEN";

#[derive(Deserialize, Debug)]
struct CloudflareDnsResult {
    pub id: String,
}

#[derive(Deserialize, Debug)]
struct CloudflareDnsResponse {
    pub result: Option<CloudflareDnsResult>,
}

#[derive(thiserror::Error, Debug)]
pub enum DnsError {
    #[error("Couldn't read env var: {0}")]
    EnvVar(#[from] std::env::VarError),
    #[error("Couldn't parse int val: {0}")]
    IntValue(#[from] std::num::ParseIntError),
    #[error("Couldn't read secret key: {0}")]
    SecretKey(#[from] crate::auth::key_provider::KeyProviderError),
    #[error("Error requesting DNS entry: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Error handling JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unknown DNS error: {0}")]
    Unknown(anyhow::Error),
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

impl CloudflarePayload {
    pub fn new(node_name: String, origin_ip: String) -> DnsResult<Self> {
        let name = format!("{node_name}.{}", std::env::var(CF_DNS_BASE)?);
        let ttl: i64 = std::env::var(CF_TTL)?.parse()?;

        Ok(Self {
            r#type: "A".to_string(),
            name,
            content: origin_ip,
            ttl,
            priority: 10,
            proxied: false,
        })
    }
}

#[derive(Clone)]
pub struct CloudflareApi {
    pub base_url: String,
    pub zone_id: String,
    pub token: String,
}

impl CloudflareApi {
    pub fn new(base_url: String, zone_id: String, token: String) -> Self {
        Self {
            base_url,
            zone_id,
            token,
        }
    }

    pub fn new_from_env() -> DnsResult<Self> {
        let zone_id = std::env::var(CF_ZONE)?;
        let base_url = std::env::var(CF_BASE_URL)?;
        let token = KeyProvider::get_var(CF_TOKEN)?;

        Ok(Self::new(base_url, zone_id, token))
    }

    pub async fn get_node_dns(&self, name: String, origin_ip: String) -> DnsResult<String> {
        let payload = CloudflarePayload::new(name.clone(), origin_ip)?;
        let endpoint = format!("zones/{}/dns_records", self.zone_id);

        match self.post(payload, endpoint).await {
            Ok(response) => {
                tracing::debug!("received response: {response:?}");

                let id = response
                    .result
                    .ok_or_else(|| DnsError::Unknown(anyhow!("Response result is not parsable")))?
                    .id;
                tracing::debug!("Created DNS entry for node name '{name}': {}", id);

                Ok(id)
            }
            Err(e) => {
                tracing::error!("Couldn't create DNS entry for node '{name}': {e}",);

                Err(DnsError::Unknown(anyhow!(e)))
            }
        }
    }

    pub async fn remove_node_dns(&self, id: String) -> DnsResult<()> {
        let endpoint = format!("zones/{}/dns_records/{}", self.zone_id, id);

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
        let url = format!("{}/{}", self.base_url, endpoint);
        let client = reqwest::Client::new();

        tracing::debug!("Sending payload to cloudflare: {payload:?}");

        let res = client
            .post(url)
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .await?
            .json::<CloudflareDnsResponse>()
            .await?;

        Ok(res)
    }

    async fn delete(&self, endpoint: String) -> DnsResult<http::status::StatusCode> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let client = reqwest::Client::new();
        let res = client.delete(url).bearer_auth(&self.token).send().await?;

        Ok(res.status())
    }
}
