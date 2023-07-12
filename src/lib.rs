#![recursion_limit = "256"]

pub mod auth;
pub mod config;
pub mod cookbook;
pub mod database;
pub mod dns;
pub mod error;
pub mod grpc;
pub mod http;
pub mod hybrid_server;
pub mod mail;
pub mod models;
pub mod server;
pub mod timestamp;

use error::{Error, Result};

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use crate::cookbook::{self, Cookbook};

    pub struct TestCookbook {
        mock: mockito::ServerGuard,
    }

    struct MockStorage {}

    #[tonic::async_trait]
    impl cookbook::Client for MockStorage {
        async fn read_file(&self, _: &str, _: &str) -> crate::Result<Vec<u8>> {
            Ok(cookbook::script::TEST_SCRIPT.bytes().collect())
        }

        async fn download_url(&self, _: &str, _: &str, _: Duration) -> crate::Result<String> {
            panic!("We're not using this in tests.")
        }

        async fn list(&self, _: &str, _: &str) -> crate::Result<Vec<String>> {
            panic!("We're not using this in tests.")
        }
    }

    impl TestCookbook {
        pub async fn new() -> Self {
            let mock = Self::mock_cookbook_api().await;
            Self { mock }
        }

        pub fn get_cookbook_api(&self) -> Cookbook {
            Cookbook::new_with_client(&self.mock_config(), MockStorage {})
        }

        async fn mock_cookbook_api() -> mockito::ServerGuard {
            let mut r2_server = mockito::Server::new_async().await;
            r2_server
                .mock("POST", mockito::Matcher::Regex(r"^/*".to_string()))
                .with_status(200)
                .with_body("{\"data\":\"id\"}")
                .create_async()
                .await;
            r2_server
        }

        pub fn mock_config(&self) -> Arc<crate::config::cookbook::Config> {
            let config = crate::config::cookbook::Config {
                dir_chains_prefix: "fake".to_string(),
                r2_bucket: "news".to_string(),
                r2_url: self.mock.url().parse().unwrap(),
                presigned_url_expiration: "1d".parse().unwrap(),
                region: "eu-west-3".to_string(),
                key_id: "not actually a".parse().unwrap(),
                key: "key".parse().unwrap(),
                bundle_bucket: "bundles".to_string(),
            };
            Arc::new(config)
        }
    }
}
