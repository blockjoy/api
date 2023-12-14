use std::time::Duration;

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Error;
use aws_sdk_s3::operation::put_object::PutObjectError;
use aws_sdk_s3::presigning::{PresigningConfig, PresigningConfigError};
use aws_sdk_s3::primitives::ByteStreamError;
use displaydoc::Display;
use thiserror::Error;
use url::Url;

#[mockall::automock]
#[tonic::async_trait]
pub trait Client: Send + Sync {
    /// List entries in `path` non-recursively.
    async fn list(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error>;

    /// List all entries in `path` recursively.
    async fn list_all(&self, bucket: &str, path: &str) -> Result<Vec<String>, Error>;

    async fn read_key(&self, bucket: &str, key: &str) -> Result<Vec<u8>, Error>;

    async fn write_key(&self, bucket: &str, key: &str, data: Vec<u8>) -> Result<(), Error>;

    async fn download_url(&self, bucket: &str, key: &str, expires: Duration) -> Result<Url, Error>;

    async fn upload_url(&self, bucket: &str, key: &str, expires: Duration) -> Result<Url, Error>;
}

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create presigned download URL for key `{0}`: {1}
    DownloadUrl(String, SdkError<GetObjectError>),
    /// Failed to list path `{0}`: {1}
    ListPath(String, SdkError<ListObjectsV2Error>),
    /// Failed to parse URL from PresignedRequest: {0}
    ParseRequestUrl(url::ParseError),
    /// Failed to create presigned config: {0}
    PresigningConfig(PresigningConfigError),
    /// Failed to query key `{0}:{1}`: {2}
    QueryKey(String, String, ByteStreamError),
    /// Failed to read key `{0}:{1}`: {2}
    ReadKey(String, String, SdkError<GetObjectError>),
    #[cfg(any(test, feature = "integration-test"))]
    /// Unexpected error: {0}
    Unexpected(&'static str),
    /// Failed to create presigned download URL for key `{0}`: {1}
    UploadUrl(String, SdkError<PutObjectError>),
    /// Failed to write key `{0}:{1}`: {2}
    WriteKey(String, String, SdkError<PutObjectError>),
}

#[tonic::async_trait]
impl Client for aws_sdk_s3::Client {
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
            .iter()
            .filter_map(|object| object.prefix().map(ToString::to_string))
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
            .iter()
            .filter_map(|object| object.key().map(ToString::to_string))
            .collect();

        Ok(files)
    }

    async fn read_key(&self, bucket: &str, key: &str) -> Result<Vec<u8>, Error> {
        let key = key.to_lowercase();
        let response = self
            .get_object()
            .bucket(bucket)
            .key(&key)
            .send()
            .await
            .map_err(|err| Error::ReadKey(bucket.into(), key.clone(), err))?;

        response
            .body
            .collect()
            .await
            .map(|bytes| bytes.into_bytes().to_vec())
            .map_err(|err| Error::QueryKey(bucket.into(), key, err))
    }

    async fn write_key(&self, bucket: &str, key: &str, data: Vec<u8>) -> Result<(), Error> {
        let key = key.to_lowercase();
        let _response = self
            .put_object()
            .bucket(bucket)
            .key(&key)
            .body(data.into())
            .send()
            .await
            .map_err(|err| Error::WriteKey(bucket.into(), key.clone(), err))?;

        Ok(())
    }

    async fn download_url(&self, bucket: &str, key: &str, expires: Duration) -> Result<Url, Error> {
        let key = key.to_lowercase();
        let config = PresigningConfig::expires_in(expires).map_err(Error::PresigningConfig)?;

        self.get_object()
            .bucket(bucket)
            .key(&key)
            .presigned(config)
            .await
            .map_err(|err| Error::DownloadUrl(key, err))
            .and_then(|url| url.uri().parse().map_err(Error::ParseRequestUrl))
    }

    async fn upload_url(&self, bucket: &str, key: &str, expires: Duration) -> Result<Url, Error> {
        let key = key.to_lowercase();
        let config = PresigningConfig::expires_in(expires).map_err(Error::PresigningConfig)?;

        self.put_object()
            .bucket(bucket)
            .key(&key)
            .presigned(config)
            .await
            .map_err(|err| Error::UploadUrl(key, err))
            .and_then(|url| url.uri().parse().map_err(Error::ParseRequestUrl))
    }
}

#[cfg(test)]
mod tests {
    use mockall::predicate::eq;

    use crate::config::storage::{BucketConfig, Config};
    use crate::models::NodeType;
    use crate::storage::image::ImageId;
    use crate::storage::{manifest::DownloadManifest, Storage};

    use super::*;

    fn test_image() -> ImageId {
        ImageId::new("test_chain", NodeType::Node, "1.2.3".to_string().into())
    }

    fn dummy_config() -> Config {
        Config {
            bucket: BucketConfig {
                cookbook: "cookbook".to_string(),
                bundle: "bundle".to_string(),
                kernel: "kernel".to_string(),
                archive: "archive".to_string(),
            },
            storage_url: "https://dummy.url".parse().unwrap(),
            key_id: Default::default(),
            key: Default::default(),
            region: "eu-west-3".to_string(),
            dir_chains_prefix: "chains".to_string(),
            presigned_url_expiration: "1d".parse().unwrap(),
        }
    }

    #[tokio::test]
    async fn test_download_manifest_client_error() {
        let mut client = MockClient::new();
        client
            .expect_list()
            .with(eq("archive"), eq("test_chain/Node/"))
            .once()
            .returning(|_, _| Err(Error::Unexpected("some client error")));

        let storage = Storage::new(&dummy_config(), client);
        let result = storage.download_manifest(&test_image(), "test").await;

        assert_eq!(
            result.unwrap_err().to_string(),
            "Storage client error: Unexpected error: some client error",
        );
    }

    #[tokio::test]
    async fn test_download_manifest_no_min_versions() {
        let mut client = MockClient::new();
        client
            .expect_list()
            .with(eq("archive"), eq("test_chain/Node/"))
            .once()
            .returning(|_, _| Ok(vec![]));
        client
            .expect_list()
            .with(eq("archive"), eq("test_chain/Node/"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "test_chain/node/invalid/".to_owned(),
                    "test_chain/node/7.7.7/".to_owned(),
                    "test_chain/node/8.8.8/".to_owned(),
                ])
            });

        let storage = Storage::new(&dummy_config(), client);

        let result = storage.download_manifest(&test_image(), "test").await;
        assert_eq!(
            result.unwrap_err().to_string(),
            r#"No download manifest found for `ImageId { protocol: "test_chain", node_type: Node, node_version: NodeVersion("1.2.3") }` in network test."#,
        );

        let result = storage.download_manifest(&test_image(), "test").await;
        assert_eq!(
            result.unwrap_err().to_string(),
            r#"No download manifest found for `ImageId { protocol: "test_chain", node_type: Node, node_version: NodeVersion("1.2.3") }` in network test."#,
        );
    }

    #[tokio::test]
    async fn test_download_manifest_no_data_version() {
        let mut client = MockClient::new();
        client
            .expect_list()
            .with(eq("archive"), eq("test_chain/Node/"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "test_chain/node/invalid/".to_owned(),
                    "test_chain/node/9.0.1/".to_owned(),
                    "test_chain/node/0.0.1/".to_owned(),
                    "test_chain/node/1.2.3/".to_owned(),
                ])
            });
        client
            .expect_list()
            .with(eq("archive"), eq("test_chain/Node/1.2.3/test/"))
            .once()
            .returning(|_, _| Ok(vec![]));
        client
            .expect_list()
            .with(eq("archive"), eq("test_chain/Node/0.0.1/test/"))
            .once()
            .returning(|_, _| Ok(vec![]));

        let storage = Storage::new(&dummy_config(), client);
        let result = storage.download_manifest(&test_image(), "test").await;

        assert_eq!(
            result.unwrap_err().to_string(),
            r#"No download manifest found for `ImageId { protocol: "test_chain", node_type: Node, node_version: NodeVersion("1.2.3") }` in network test."#,
        );
    }

    #[tokio::test]
    async fn test_download_manifest_no_manifest_or_invalid() {
        let mut client = MockClient::new();
        client
            .expect_list()
            .with(eq("archive"), eq("test_chain/Node/"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "test_chain/node/invalid/".to_owned(),
                    "test_chain/node/9.0.1/".to_owned(),
                    "test_chain/node/1.2.3/".to_owned(),
                ])
            });
        client
            .expect_list()
            .with(eq("archive"), eq("test_chain/Node/1.2.3/test/"))
            .once()
            .returning(|_, _| {
                Ok(vec![
                    "test_chain/node/1.2.3/test/invalid/".to_owned(),
                    "test_chain/node/1.2.3/test/1/".to_owned(),
                    "test_chain/node/1.2.3/test/2/".to_owned(),
                ])
            });
        client
            .expect_read_key()
            .once()
            .returning(|_, _| Err(Error::Unexpected("no file")));
        client
            .expect_read_key()
            .once()
            .returning(|_, _| Ok(b"invalid manifest content".to_vec()));

        let storage = Storage::new(&dummy_config(), client);
        let result = storage.download_manifest(&test_image(), "test").await;

        assert_eq!(
            result.unwrap_err().to_string(),
            r#"No download manifest found for `ImageId { protocol: "test_chain", node_type: Node, node_version: NodeVersion("1.2.3") }` in network test."#,
        );
    }

    #[tokio::test]
    async fn test_download_manifest_ok() {
        let mut client = MockClient::new();
        client
            .expect_list()
            .with(eq("archive"), eq("test_chain/Node/"))
            .once()
            .returning(|_, _| Ok(vec!["test_chain/node/1.1.1/".to_owned()]));
        client
            .expect_list()
            .with(eq("archive"), eq("test_chain/Node/1.1.1/test/"))
            .once()
            .returning(|_, _| Ok(vec!["test_chain/node/1.1.1/test/2/".to_owned()]));
        client
            .expect_read_key()
            .once()
            .returning(|_, _| Ok(br#"{"total_size": 128,"chunks": []}"#.to_vec()));

        let storage = Storage::new(&dummy_config(), client);
        let manifest = storage
            .download_manifest(&test_image(), "test")
            .await
            .unwrap();

        assert_eq!(
            manifest,
            DownloadManifest {
                total_size: 128,
                compression: None,
                chunks: vec![],
            }
        );
    }
}
