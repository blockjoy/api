use std::time::Duration;

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Error;
use aws_sdk_s3::presigning::{PresigningConfig, PresigningConfigError};
use aws_sdk_s3::primitives::ByteStreamError;
use displaydoc::Display;
use thiserror::Error;

#[tonic::async_trait]
pub trait Client: Send + Sync {
    async fn read_file(&self, bucket: &str, path: &str) -> Result<Vec<u8>, Error>;

    async fn read_string(&self, bucket: &str, path: &str) -> Result<String, Error> {
        self.read_file(bucket, path)
            .await
            .and_then(|bytes| String::from_utf8(bytes).map_err(Error::ParseUtf8))
    }

    async fn download_url(
        &self,
        bucket: &str,
        path: &str,
        expiration: Duration,
    ) -> Result<String, Error>;

    /// List entries in given `path` non-recursively.
    async fn list(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error>;

    /// List all entries in given `path` recursively.
    async fn list_all(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error>;
}

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to list path `{0}`: {1}
    ListPath(String, SdkError<ListObjectsV2Error>),
    /// Failed to parse bytes as UTF8: {0}
    ParseUtf8(std::string::FromUtf8Error),
    /// Failed to create presigned config: {0}
    PresignedConfig(PresigningConfigError),
    /// Failed to create presigned URL for path `{0}`: {1}
    PresignedUrl(String, SdkError<GetObjectError>),
    /// Failed to query file `{0}:{1}`: {2}
    QueryFile(String, String, ByteStreamError),
    /// Failed to read file `{0}:{1}`: {2}
    ReadFile(String, String, SdkError<GetObjectError>),
    #[cfg(any(test, feature = "integration-test"))]
    /// Unexpected error: {0}
    Unexpected(&'static str),
}

#[tonic::async_trait]
impl Client for aws_sdk_s3::Client {
    async fn read_file(&self, bucket: &str, path: &str) -> Result<Vec<u8>, Error> {
        let path = path.to_lowercase();
        let response = self
            .get_object()
            .bucket(bucket)
            .key(&path)
            .send()
            .await
            .map_err(|err| Error::ReadFile(bucket.into(), path.clone(), err))?;

        response
            .body
            .collect()
            .await
            .map(|bytes| bytes.into_bytes().to_vec())
            .map_err(|err| Error::QueryFile(bucket.into(), path, err))
    }

    async fn download_url(
        &self,
        bucket: &str,
        path: &str,
        expires: Duration,
    ) -> Result<String, Error> {
        let path = path.to_lowercase();
        let presigned = PresigningConfig::expires_in(expires).map_err(Error::PresignedConfig)?;

        self.get_object()
            .bucket(bucket)
            .key(&path)
            .presigned(presigned)
            .await
            .map(|url| url.uri().to_string())
            .map_err(|err| Error::PresignedUrl(path, err))
    }

    async fn list(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error> {
        let path = path.to_lowercase();
        let resp = self
            .list_objects_v2()
            .bucket(bucket)
            .prefix(&path)
            .delimiter('/')
            .send()
            .await
            .map_err(|err| Error::ListPath(path, err))?;

        let files = resp
            .common_prefixes()
            .unwrap_or_default()
            .iter()
            .filter_map(|object| object.prefix().map(ToOwned::to_owned))
            .collect();

        Ok(files)
    }

    async fn list_all(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error> {
        let path = path.to_lowercase();
        let resp = self
            .list_objects_v2()
            .bucket(bucket)
            .prefix(&path)
            .send()
            .await
            .map_err(|err| Error::ListPath(path, err))?;

        let files = resp
            .contents()
            .unwrap_or_default()
            .iter()
            .filter_map(|object| object.key().map(ToOwned::to_owned))
            .collect();

        Ok(files)
    }
}

#[cfg(any(test, feature = "integration-test"))]
mod tests {
    use crate::cookbook::identifier::Identifier;
    use crate::cookbook::script::tests::TEST_SCRIPT;
    use crate::cookbook::tests::MockStorage;
    use crate::models::NodeType;

    #[cfg(test)]
    use {
        crate::cookbook::{tests::dummy_config, Cookbook},
        crate::grpc::api,
        mockall::predicate::eq,
    };

    use super::*;

    mockall::mock! {
        pub Client {}

        #[tonic::async_trait]
        impl Client for Client {
            async fn read_file(&self, bucket: &str, path: &str) -> Result<Vec<u8>, Error>;
            async fn download_url(&self, bucket: &str, path: &str, expiration: Duration) -> Result<String, Error>;
            async fn list(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error>;
            async fn list_all(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error>;
        }
    }

    #[tonic::async_trait]
    impl Client for MockStorage {
        async fn read_file(&self, _: &str, _: &str) -> Result<Vec<u8>, Error> {
            Ok(TEST_SCRIPT.bytes().collect())
        }

        async fn download_url(&self, _: &str, _: &str, _: Duration) -> Result<String, Error> {
            panic!("We're not using this in tests.")
        }

        async fn list(&self, _: &str, _: &str) -> Result<Vec<String>, Error> {
            panic!("We're not using this in tests.")
        }

        async fn list_all(&self, _: &str, _: &str) -> Result<Vec<String>, Error> {
            panic!("We're not using this in tests.")
        }
    }

    #[allow(dead_code)]
    fn test_identifier() -> Identifier {
        Identifier::new(
            "test_blockchain",
            NodeType::Node,
            "1.2.3".to_string().into(),
        )
    }

    #[tokio::test]
    async fn test_get_download_manifest_client_error() {
        let mut client = MockClient::new();

        client
            .expect_list()
            .with(eq("archive"), eq("test_blockchain/Node/"))
            .once()
            .returning(|_, _| Err(Error::Unexpected("some client error")));

        let cookbook = Cookbook::new(&dummy_config(), client);

        assert_eq!(
            "Cookbook client error: Unexpected error: some client error",
            cookbook
                .get_download_manifest(&test_identifier(), "test")
                .await
                .unwrap_err()
                .to_string()
        );
    }

    #[tokio::test]
    async fn test_get_download_manifest_no_min_versions() {
        let mut client = MockClient::new();

        client
            .expect_list()
            .with(eq("archive"), eq("test_blockchain/Node/"))
            .once()
            .returning(|_, _| Ok(vec![]));
        client
            .expect_list()
            .with(eq("archive"), eq("test_blockchain/Node/"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "test_blockchain/node/invalid/".to_owned(),
                    "test_blockchain/node/7.7.7/".to_owned(),
                    "test_blockchain/node/8.8.8/".to_owned(),
                ])
            });

        let cookbook = Cookbook::new(&dummy_config(), client);

        assert_eq!(
            r#"No manifest found for `Identifier { protocol: "test_blockchain", node_type: Node, node_version: NodeVersion("1.2.3") }` in network test."#,
            cookbook
                .get_download_manifest(&test_identifier(), "test")
                .await
                .unwrap_err()
                .to_string()
        );
        assert_eq!(
            r#"No manifest found for `Identifier { protocol: "test_blockchain", node_type: Node, node_version: NodeVersion("1.2.3") }` in network test."#,
            cookbook
                .get_download_manifest(&test_identifier(), "test")
                .await
                .unwrap_err()
                .to_string()
        );
    }

    #[tokio::test]
    async fn test_get_download_manifest_no_data_version() {
        let mut client = MockClient::new();

        client
            .expect_list()
            .with(eq("archive"), eq("test_blockchain/Node/"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "test_blockchain/node/invalid/".to_owned(),
                    "test_blockchain/node/9.0.1/".to_owned(),
                    "test_blockchain/node/0.0.1/".to_owned(),
                    "test_blockchain/node/1.2.3/".to_owned(),
                ])
            });
        client
            .expect_list()
            .with(eq("archive"), eq("test_blockchain/Node/1.2.3/test/"))
            .once()
            .returning(|_, _| Ok(vec![]));
        client
            .expect_list()
            .with(eq("archive"), eq("test_blockchain/Node/0.0.1/test/"))
            .once()
            .returning(|_, _| Ok(vec![]));

        let cookbook = Cookbook::new(&dummy_config(), client);

        assert_eq!(
            r#"No manifest found for `Identifier { protocol: "test_blockchain", node_type: Node, node_version: NodeVersion("1.2.3") }` in network test."#,
            cookbook
                .get_download_manifest(&test_identifier(), "test")
                .await
                .unwrap_err()
                .to_string()
        );
    }

    #[tokio::test]
    async fn test_get_download_manifest_no_manifest_or_invalid() {
        let mut client = MockClient::new();

        client
            .expect_list()
            .with(eq("archive"), eq("test_blockchain/Node/"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "test_blockchain/node/invalid/".to_owned(),
                    "test_blockchain/node/9.0.1/".to_owned(),
                    "test_blockchain/node/1.2.3/".to_owned(),
                ])
            });
        client
            .expect_list()
            .with(eq("archive"), eq("test_blockchain/Node/1.2.3/test/"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "test_blockchain/node/1.2.3/test/invalid/".to_owned(),
                    "test_blockchain/node/1.2.3/test/1/".to_owned(),
                    "test_blockchain/node/1.2.3/test/2/".to_owned(),
                ])
            });
        client
            .expect_read_file()
            .once()
            .returning(|_, _| Err(Error::Unexpected("no file")));
        client
            .expect_read_file()
            .once()
            .returning(|_, _| Ok(b"invalid manifest content".to_vec()));

        let cookbook = Cookbook::new(&dummy_config(), client);

        assert_eq!(
            r#"No manifest found for `Identifier { protocol: "test_blockchain", node_type: Node, node_version: NodeVersion("1.2.3") }` in network test."#,
            cookbook
                .get_download_manifest(&test_identifier(), "test")
                .await
                .unwrap_err()
                .to_string()
        );
    }

    #[tokio::test]
    async fn test_get_download_manifest_ok() {
        let mut client = MockClient::new();

        client
            .expect_list()
            .with(eq("archive"), eq("test_blockchain/Node/"))
            .once()
            .returning(|_, _| Ok(vec!["test_blockchain/node/1.1.1/".to_owned()]));
        client
            .expect_list()
            .with(eq("archive"), eq("test_blockchain/Node/1.1.1/test/"))
            .once()
            .returning(|_, _| Ok(vec!["test_blockchain/node/1.1.1/test/2/".to_owned()]));
        client
            .expect_read_file()
            .once()
            .returning(|_, _| Ok(br#"{"total_size": 128,"chunks": []}"#.to_vec()));

        let cookbook = Cookbook::new(&dummy_config(), client);
        let manifest = cookbook
            .get_download_manifest(&test_identifier(), "test")
            .await
            .unwrap();

        assert_eq!(
            manifest,
            api::DownloadManifest {
                total_size: 128,
                compression: None,
                chunks: vec![],
            }
        );
    }
}
