pub mod api;

pub mod client;
pub use client::Client;

use std::net::IpAddr;
use std::sync::Arc;

use displaydoc::Display;
use thiserror::Error;

use crate::config::cloudflare::Config;

use self::api::dns::{
    CreateDnsRecord, CreateDnsRecordParams, DeleteDnsRecord, DnsContent, DnsRecord,
};

#[tonic::async_trait]
pub trait Dns {
    async fn create(&self, name: &str, ip: IpAddr) -> Result<DnsRecord, Error>;

    async fn delete(&self, id: &str) -> Result<(), Error>;
}

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create cloudflare Client: {0}
    CreateClient(client::Error),
    /// Failed to create cloudflare DNS record `{0}`: {1}
    CreateDns(String, client::Error),
    /// Failed to delete cloudflare DNS record `{0}`: {1}
    DeleteDns(String, client::Error),
}

pub struct Cloudflare {
    pub config: Arc<Config>,
    pub client: Client,
}

impl Cloudflare {
    pub fn new(config: Arc<Config>) -> Result<Self, Error> {
        let client = Client::new(&config.api.token).map_err(Error::CreateClient)?;

        Ok(Cloudflare { config, client })
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn new_mock(config: Arc<Config>, server_url: url::Url) -> Result<Self, Error> {
        let client = Client::new_mock(server_url).map_err(Error::CreateClient)?;

        Ok(Cloudflare { config, client })
    }

    pub async fn create_dns(&self, name: &str, ip: IpAddr) -> Result<DnsRecord, Error> {
        let name = format!("{name}.{}", self.config.dns.base);
        let content = match ip {
            IpAddr::V4(ip) => DnsContent::A { content: ip },
            IpAddr::V6(ip) => DnsContent::AAAA { content: ip },
        };

        let endpoint = CreateDnsRecord {
            zone_identifier: &self.config.api.zone_id,
            params: CreateDnsRecordParams {
                ttl: Some(self.config.dns.ttl),
                priority: Some(10),
                proxied: Some(false),
                name: &name,
                content,
            },
        };

        self.client
            .request(&endpoint)
            .await
            .map_err(|err| Error::CreateDns(name, err))
    }

    pub async fn delete_dns(&self, id: &str) -> Result<(), Error> {
        let endpoint = DeleteDnsRecord {
            zone_identifier: &self.config.api.zone_id,
            identifier: id,
        };

        self.client
            .request(&endpoint)
            .await
            .map(|_resp| ())
            .map_err(|err| Error::DeleteDns(id.to_string(), err))
    }
}

#[tonic::async_trait]
impl Dns for Cloudflare {
    async fn create(&self, name: &str, ip: IpAddr) -> Result<DnsRecord, Error> {
        self.create_dns(name, ip).await
    }

    async fn delete(&self, id: &str) -> Result<(), Error> {
        self.delete_dns(id).await
    }
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use std::net::Ipv4Addr;
    use std::sync::Arc;

    use chrono::Utc;
    use mockito::{Matcher, ServerGuard};
    use rand::rngs::OsRng;
    use rand::Rng;

    use crate::config::cloudflare::{ApiConfig, Config, DnsConfig};

    use super::api::dns::Meta;
    use super::api::ApiSuccess;
    use super::*;

    pub struct MockCloudflare {
        pub server: ServerGuard,
        pub cloudflare: Cloudflare,
    }

    impl MockCloudflare {
        pub async fn new(rng: &mut OsRng) -> Self {
            let id = rng.gen_range(200_000..5_000_000);
            let server = mock_server(id).await;
            let server_url = server.url().parse().unwrap();
            let config = Arc::new(mock_config(&server));
            let cloudflare = Cloudflare::new_mock(config, server_url).unwrap();

            MockCloudflare { server, cloudflare }
        }
    }

    #[tonic::async_trait]
    impl Dns for MockCloudflare {
        async fn create(&self, name: &str, ip: IpAddr) -> Result<DnsRecord, Error> {
            self.cloudflare.create_dns(name, ip).await
        }

        async fn delete(&self, id: &str) -> Result<(), Error> {
            self.cloudflare.delete_dns(id).await
        }
    }

    async fn mock_server(id: u32) -> ServerGuard {
        let mut server = mockito::Server::new_async().await;

        server
            .mock("POST", Matcher::Regex(r"^/zones/.*/dns_records$".into()))
            .with_status(200)
            .with_body(serde_json::to_string(&mock_dns_record(id)).unwrap())
            .create_async()
            .await;

        server
            .mock(
                "DELETE",
                Matcher::Regex(r"^/zones/.*/dns_records/.*$".into()),
            )
            .with_status(200)
            .create_async()
            .await;

        server
    }

    fn mock_config(_: &ServerGuard) -> Config {
        Config {
            api: ApiConfig {
                zone_id: "zone_id".into(),
                token: "token".parse().unwrap(),
            },
            dns: DnsConfig {
                base: "base".into(),
                ttl: 3600,
            },
        }
    }

    fn mock_dns_record(id: u32) -> ApiSuccess<DnsRecord> {
        ApiSuccess {
            result: DnsRecord {
                meta: Meta { auto_added: false },
                locked: None,
                name: "test".into(),
                ttl: 1,
                zone_id: "zone_id".into(),
                modified_on: Utc::now(),
                created_on: Utc::now(),
                proxiable: false,
                content: DnsContent::A {
                    content: Ipv4Addr::LOCALHOST,
                },
                id: format!("{id:x}"),
                proxied: false,
                zone_name: Some("zone".into()),
            },
            result_info: None,
            messages: serde_json::Value::Null,
            errors: vec![],
        }
    }

    #[tokio::test]
    async fn test_parse_dns() {
        let test1 = r#"{"result":{"id":"45afecb529c9029d909e1a2ca863fd9d","name":"formally-knowing-eel.n0des.xyz","type":"A","content":"127.0.0.8","proxiable":false,"proxied":false,"ttl":300,"settings":{},"meta":{"auto_added":false,"managed_by_apps":false,"managed_by_argo_tunnel":false},"comment":null,"tags":[],"created_on":"2025-01-27T16:29:07.984046Z","modified_on":"2025-01-27T16:29:07.984046Z"},"success":true,"errors":[],"messages":[]}"#;
        let _: ApiSuccess<DnsRecord> = serde_json::from_str(test1).unwrap();
    }

    #[tokio::test]
    async fn create_dns_record() {
        let (ctx, _db) = crate::config::Context::with_mocked().await.unwrap();
        let name = petname::petname(3, "-").unwrap();

        ctx.dns
            .create(&name, Ipv4Addr::LOCALHOST.into())
            .await
            .unwrap();
    }
}
