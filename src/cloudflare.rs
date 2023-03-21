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

#[derive(Deserialize, Debug)]
struct CloudflareDnsResult {
    pub id: String,
}

#[derive(Deserialize, Debug)]
struct CloudflareDnsResponse {
    pub errors: serde_json::Value,
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

#[derive(Serialize)]
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
        let name = format!("{node_name}.{}", std::env::var("CF_DNS_BASE")?);
        let ttl: i64 = std::env::var("CF_TTL")?.parse()?;

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

pub struct CloudflareApi {
    pub base_url: String,
    pub zone_id: String,
    pub token: String,
    pub origin_ip: String,
}

impl CloudflareApi {
    pub fn new(origin_ip: String) -> DnsResult<Self> {
        let zone_id = std::env::var("CF_ZONE")?;
        let base_url = std::env::var("CF_BASE_URL")?;
        let token = KeyProvider::get_var("CF_TOKEN")?.value;

        Ok(Self {
            base_url,
            zone_id,
            token,
            origin_ip,
        })
    }

    pub async fn get_node_dns(&self, name: String, origin_ip: String) -> DnsResult<String> {
        let payload = CloudflarePayload::new(name.clone(), origin_ip)?;
        let endpoint = format!("zones/{}/dns_records", self.zone_id);
        let response = self.post(payload, endpoint).await?;

        if response.result.is_some() {
            let id = response.result.unwrap().id;
            tracing::debug!("Created DNS entry for node name '{name}': {}", id);

            Ok(id)
        } else {
            tracing::error!(
                "Couldn't create DNS entry for node '{name}': {:?}",
                response.errors
            );
            Err(DnsError::Unknown(anyhow!(response.errors)))
        }
    }

    pub async fn remove_node_dns(&self, id: String) -> DnsResult<bool> {
        let endpoint = format!("zones/{}/dns_records/{}", self.zone_id, id);

        Ok(self.delete(endpoint).await? == http::status::StatusCode::OK)
    }

    async fn post(
        &self,
        payload: CloudflarePayload,
        endpoint: String,
    ) -> DnsResult<CloudflareDnsResponse> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let client = reqwest::Client::new();
        let res = client
            .post(url)
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .await?
            .json::<CloudflareDnsResponse>()
            .await?;

        dbg!(&res);

        Ok(res)
    }

    async fn delete(&self, endpoint: String) -> DnsResult<http::status::StatusCode> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let client = reqwest::Client::new();
        let res = client.delete(url).bearer_auth(&self.token).send().await?;

        dbg!(&res);

        Ok(res.status())
    }
}
