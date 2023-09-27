pub mod cloudflare;
pub use cloudflare::Cloudflare;

use displaydoc::Display;
use thiserror::Error;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to delete Cloudflare endpoint `{0}`: {1}
    DeleteEndpoint(String, reqwest::Error),
    /// Response is missing `result`.
    MissingResult,
    /// Failed to send post request to Cloudflare: {0}
    PostRequest(reqwest::Error),
    /// Failed to parser post response from Cloudflare: {0}
    PostResponse(reqwest::Error),
    /// Unknown DNS error: {0}
    Unknown(anyhow::Error),
}

#[tonic::async_trait]
pub trait Dns {
    async fn get_node_dns(&self, node_name: &str, origin_ip: String) -> Result<String, Error>;

    async fn remove_node_dns(&self, id: &str) -> Result<(), Error>;
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use std::sync::Arc;

    use mockito::{Matcher, ServerGuard};
    use rand::Rng;

    use crate::config::cloudflare::{ApiConfig, Config, DnsConfig};
    #[allow(unused_imports)]
    use crate::config::Context;

    use super::*;

    pub struct MockDns {
        pub server: ServerGuard,
        pub config: Arc<Config>,
        pub cloudflare: Cloudflare,
    }

    impl MockDns {
        pub async fn new() -> Self {
            let mut server = mockito::Server::new_async().await;

            let mut rng = rand::thread_rng();
            let id_dns = rng.gen_range(200_000..5_000_000);

            server
                .mock("POST", Matcher::Regex(r"^/zones/.*/dns_records$".into()))
                .with_status(200)
                .with_body(format!("{{\"result\":{{\"id\":\"{:x}\"}}}}", id_dns))
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

            let config = Arc::new(Self::mock_config(&server));
            let cloudflare = Cloudflare::new(config.clone());

            MockDns {
                server,
                config,
                cloudflare,
            }
        }

        pub fn mock_config(server: &ServerGuard) -> Config {
            Config {
                api: ApiConfig {
                    base_url: server.url(),
                    zone_id: "zone_id".into(),
                    token: "token".parse().unwrap(),
                },
                dns: DnsConfig {
                    base: "base".into(),
                    ttl: 3600,
                },
            }
        }
    }

    #[tonic::async_trait]
    impl Dns for MockDns {
        async fn get_node_dns(&self, node_name: &str, origin_ip: String) -> Result<String, Error> {
            self.cloudflare.get_node_dns(node_name, origin_ip).await
        }

        async fn remove_node_dns(&self, id: &str) -> Result<(), Error> {
            self.cloudflare.remove_node_dns(id).await
        }
    }

    #[tokio::test]
    async fn can_create_node_dns() {
        let (ctx, _db) = Context::with_mocked().await.unwrap();
        let name = format!("test_{}", petname::petname(3, "_").unwrap());

        let id = ctx
            .dns
            .get_node_dns(&name, "127.0.0.1".to_string())
            .await
            .unwrap();
        assert!(!id.is_empty());
    }
}
