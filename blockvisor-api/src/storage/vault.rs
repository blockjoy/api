use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

use displaydoc::Display;
use rustify::errors::ClientError as RestClientError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time::{self, Instant};
use tonic::Status;
use tracing::warn;
use vaultrs::client::{Client, VaultClient, VaultClientSettingsBuilder};
use vaultrs::error::ClientError;
use vaultrs::kv2;

use crate::config::vault::Config;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to build vault client: {0}
    BuildClient(ClientError),
    /// Failed to build vault client settings: {0}
    BuildSettings(vaultrs::client::VaultClientSettingsBuilderError),
    /// Failed to connect to vault: {0}
    Connection(Box<Error>),
    /// Failed to delete KV path `{0}`: {1}
    DeletePath(String, ClientError),
    /// Failed to get KV path `{0}`: {1}
    GetPath(String, ClientError),
    /// Failed to list KV path `{0}`: {1}
    ListPath(String, ClientError),
    /// No vault access token or token file provided.
    NoTokenOrFile,
    /// The requested path was not found.
    PathNotFound,
    /// Failed to read token file at `{0}`: `{1}`
    ReadTokenFile(String, std::io::Error),
    /// Failed to set KV path `{0}`: {1}
    SetPath(String, ClientError),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            BuildClient(_)
            | BuildSettings(_)
            | Connection(_)
            | DeletePath(_, _)
            | GetPath(_, _)
            | ListPath(_, _)
            | NoTokenOrFile
            | ReadTokenFile(_, _)
            | SetPath(_, _) => Status::internal("Internal error."),
            PathNotFound => Status::not_found("Not found."),
        }
    }
}

pub struct Vault {
    config: Arc<Config>,
    client: VaultClient,
}

impl Vault {
    pub async fn new(config: Arc<Config>) -> Result<Arc<RwLock<Self>>, Error> {
        let mut builder = VaultClientSettingsBuilder::default();
        builder.address(config.server_address.clone());

        let (refresh_file, interval) = if let Some(token) = &config.access_token {
            builder.token(token.to_string());
            (None, *config.refresh_token_file)
        } else if let Some(file) = &config.access_token_file {
            builder.token(read_token_file(file).await?);
            (Some(file.clone()), *config.refresh_token_file)
        } else {
            return Err(Error::NoTokenOrFile);
        };

        if let Some(namespace) = &config.namespace {
            builder.set_namespace(namespace.clone());
        }

        let settings = builder.build().map_err(Error::BuildSettings)?;
        let client = VaultClient::new(settings).map_err(Error::BuildClient)?;
        let vault = Vault { config, client };

        // confirm that vault can connect to the mount point
        let vault = match vault.get_bytes("unknown").await {
            Ok(_) | Err(Error::PathNotFound) => Ok(Arc::new(RwLock::new(vault))),
            Err(err) => return Err(Error::Connection(Box::new(err))),
        }?;

        // kick off a timer to read the latest vault token from a file
        if let Some(file) = refresh_file {
            let vault = vault.clone();
            tokio::spawn(async move {
                let mut timer = time::interval_at(Instant::now() + interval, interval);

                loop {
                    timer.tick().await;
                    match read_token_file(&file).await {
                        Ok(token) => vault.write().await.client.set_token(&token),
                        Err(err) => warn!("Failed to refresh vault token: {err}"),
                    }
                }
            });
        }

        Ok(vault)
    }

    pub async fn get_path<D: DeserializeOwned>(&self, path: &str) -> Result<D, Error> {
        kv2::read(&self.client, &self.config.kv_mount, path)
            .await
            .map_err(|err| match err {
                ClientError::APIError { code: 404, .. }
                | ClientError::RestClientError {
                    source: RestClientError::ServerResponseError { code: 404, .. },
                } => Error::PathNotFound,
                _ => Error::GetPath(path.into(), err),
            })
    }

    pub async fn get_bytes(&self, path: &str) -> Result<Vec<u8>, Error> {
        self.get_path::<Bytes<'static>>(path)
            .await
            .map(|bytes| bytes.value.into_owned())
    }

    pub async fn set_path<D>(&self, path: &str, data: &D) -> Result<u64, Error>
    where
        D: Serialize + Send + Sync,
    {
        kv2::set(&self.client, &self.config.kv_mount, path, data)
            .await
            .map(|meta| meta.version)
            .map_err(|err| Error::SetPath(path.into(), err))
    }

    pub async fn set_bytes(&self, path: &str, data: &[u8]) -> Result<u64, Error> {
        let bytes = Bytes { value: data.into() };
        self.set_path(path, &bytes).await
    }

    pub async fn list_path(&self, path: &str) -> Result<Option<Vec<String>>, Error> {
        kv2::list(&self.client, &self.config.kv_mount, path)
            .await
            .map(Some)
            .or_else(|err| match err {
                ClientError::APIError { code: 404, .. }
                | ClientError::RestClientError {
                    source: RestClientError::ServerResponseError { code: 404, .. },
                } => Ok(None),
                _ => Err(Error::ListPath(path.into(), err)),
            })
    }

    /// Soft-delete the latest version.
    pub async fn delete_path(&self, path: &str) -> Result<(), Error> {
        kv2::delete_latest(&self.client, &self.config.kv_mount, path)
            .await
            .map_err(|err| match err {
                ClientError::APIError { code: 404, .. }
                | ClientError::RestClientError {
                    source: RestClientError::ServerResponseError { code: 404, .. },
                } => Error::PathNotFound,
                _ => Error::DeletePath(path.into(), err),
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bytes<'b> {
    pub value: Cow<'b, [u8]>,
}

async fn read_token_file<P>(path: P) -> Result<String, Error>
where
    P: AsRef<Path> + Send + Sync,
{
    let path = path.as_ref();
    tokio::fs::read_to_string(path)
        .await
        .map_err(|err| Error::ReadTokenFile(path.to_string_lossy().to_string(), err))
}
