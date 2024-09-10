use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use super::provider::{self, Provider};
use super::{HumanTime, Redacted};

const SERVER_ADDRESS_VAR: &str = "VAULT_SERVER_ADDRESS";
const SERVER_ADDRESS_ENTRY: &str = "vault.server_address";
const NAMESPACE_VAR: &str = "VAULT_NAMESPACE";
const NAMESPACE_ENTRY: &str = "vault.namespace";
const ACCESS_TOKEN_VAR: &str = "VAULT_ACCESS_TOKEN";
const ACCESS_TOKEN_ENTRY: &str = "vault.access_token";
const ACCESS_TOKEN_FILE_VAR: &str = "VAULT_ACCESS_TOKEN_FILE";
const ACCESS_TOKEN_FILE_ENTRY: &str = "vault.access_token_file";
const REFRESH_TOKEN_FILE_VAR: &str = "REFRESH_ACCESS_TOKEN_FILE";
const REFRESH_TOKEN_FILE_ENTRY: &str = "vault.refresh_token_file";
const REFRESH_TOKEN_FILE_DEFAULT: &str = "1h";
const KV_MOUNT_VAR: &str = "VAULT_KV_MOUNT";
const KV_MOUNT_ENTRY: &str = "vault.kv_mount";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to read {ACCESS_TOKEN_ENTRY:?}: {0}
    ReadAccessToken(provider::Error),
    /// Failed to read {ACCESS_TOKEN_FILE_ENTRY:?}: {0}
    ReadAccessTokenFile(provider::Error),
    /// Failed to read {KV_MOUNT_ENTRY:?}: {0}
    ReadKvMount(provider::Error),
    /// Failed to read {NAMESPACE_ENTRY:?}: {0}
    ReadNamespace(provider::Error),
    /// Failed to read {REFRESH_TOKEN_FILE_ENTRY:?}: {0}
    ReadRefreshTokenFile(provider::Error),
    /// Failed to read {SERVER_ADDRESS_ENTRY:?}: {0}
    ReadServerAddress(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server_address: Url,
    pub namespace: Option<String>,
    pub access_token: Option<Redacted<String>>,
    pub access_token_file: Option<String>,
    pub refresh_token_file: HumanTime,
    pub kv_mount: String,
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(Config {
            server_address: provider
                .read(SERVER_ADDRESS_VAR, SERVER_ADDRESS_ENTRY)
                .map_err(Error::ReadServerAddress)?,
            namespace: provider
                .maybe_read(NAMESPACE_VAR, NAMESPACE_ENTRY)
                .map_err(Error::ReadNamespace)?,
            access_token: provider
                .maybe_read(ACCESS_TOKEN_VAR, ACCESS_TOKEN_ENTRY)
                .map_err(Error::ReadAccessToken)?,
            access_token_file: provider
                .maybe_read(ACCESS_TOKEN_FILE_VAR, ACCESS_TOKEN_FILE_ENTRY)
                .map_err(Error::ReadAccessTokenFile)?,
            refresh_token_file: provider
                .read_or_else(
                    || REFRESH_TOKEN_FILE_DEFAULT.parse::<HumanTime>(),
                    REFRESH_TOKEN_FILE_VAR,
                    REFRESH_TOKEN_FILE_ENTRY,
                )
                .map_err(Error::ReadRefreshTokenFile)?,
            kv_mount: provider
                .read(KV_MOUNT_VAR, KV_MOUNT_ENTRY)
                .map_err(Error::ReadKvMount)?,
        })
    }
}
