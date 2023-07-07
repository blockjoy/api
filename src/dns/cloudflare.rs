//!
//! curl -X POST "https://api.cloudflare.com/client/v4/zones/89560cdd783e35f7a9d718755ea9c656/dns_records" \
//!      -H "Authorization: Bearer 9QjEiXC4B26tgshHZjuZ57kJcjaChSSsDfzUvfYQ" \
//!      -H "Content-Type: application/json" \
//!      --data '{"type":"A","name":"meow.n0des.xyz","content":"127.0.0.1","ttl":3600,"priority":10,"proxied":false,"comment":"Chain node record", "tags": ["owner": <guid>]}'
//!

use std::sync::Arc;

use hyper::http::status::StatusCode;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::config::cloudflare::Config;

use super::{Dns, Error};

#[derive(Debug, Deserialize)]
struct DnsResponse {
    pub result: Option<DnsResult>,
}

#[derive(Debug, Deserialize)]
struct DnsResult {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct Payload {
    pub r#type: String,
    pub name: String,
    pub content: String,
    pub ttl: i64,
    pub priority: i32,
    pub proxied: bool,
}

#[derive(Clone)]
pub struct Cloudflare {
    pub config: Arc<Config>,
    pub client: Arc<Client>,
}

impl Cloudflare {
    pub fn new(config: Arc<Config>) -> Self {
        let client = Arc::new(Client::new());
        Self { config, client }
    }

    pub fn payload(&self, node_name: &str, origin_ip: String) -> Payload {
        Payload {
            r#type: "A".to_string(),
            name: format!("{node_name}.{}", self.config.dns.base),
            content: origin_ip,
            ttl: self.config.dns.ttl,
            priority: 10,
            proxied: false,
        }
    }

    async fn post(&self, payload: Payload, endpoint: &str) -> Result<DnsResponse, Error> {
        debug!("Sending payload to cloudflare: {payload:?}");
        let url = format!("{}/{}", self.config.api.base_url, endpoint);

        self.client
            .post(url)
            .bearer_auth(self.config.api.token.as_str())
            .json(&payload)
            .send()
            .await
            .map_err(Error::PostRequest)?
            .json()
            .await
            .map_err(Error::PostResponse)
    }

    async fn delete(&self, endpoint: &str) -> Result<StatusCode, Error> {
        let url = format!("{}/{}", self.config.api.base_url, endpoint);
        self.client
            .delete(url)
            .bearer_auth(self.config.api.token.as_str())
            .send()
            .await
            .map(|resp| resp.status())
            .map_err(|err| Error::DeleteEndpoint(endpoint.into(), err))
    }
}

#[tonic::async_trait]
impl Dns for Cloudflare {
    async fn get_node_dns(&self, node_name: &str, origin_ip: String) -> Result<String, Error> {
        let payload = self.payload(node_name, origin_ip);
        let endpoint = format!("zones/{}/dns_records", self.config.api.zone_id);

        self.post(payload, &endpoint)
            .await
            .and_then(|resp| {
                debug!("received response: {resp:?}");
                let id = resp.result.ok_or(Error::MissingResult)?.id;
                debug!("Created DNS entry for node `{node_name}`: {}", id);
                Ok(id)
            })
            .map_err(|err| {
                error!("Couldn't create DNS entry for node `{node_name}`: {err}");
                err
            })
    }

    async fn remove_node_dns(&self, id: &str) -> Result<(), Error> {
        let endpoint = format!("zones/{}/dns_records/{}", self.config.api.zone_id, id);

        self.delete(&endpoint).await.map(|_| ()).map_err(|err| {
            error!("Couldn't delete DNS entry: {err}");
            err
        })
    }
}
