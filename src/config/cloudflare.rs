use derive_more::{Deref, FromStr};
use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;

use super::provider::{self, Provider};
use super::Redacted;

const DNS_BASE_VAR: &str = "CF_DNS_BASE";
const DNS_BASE_ENTRY: &str = "cloudflare.dns.base";
const DNS_TTL_VAR: &str = "CF_TTL";
const DNS_TTL_ENTRY: &str = "cloudflare.dns.ttl";

const API_ZONE_ID_VAR: &str = "CF_ZONE";
const API_ZONE_ID_ENTRY: &str = "cloudflare.api.zone_id";
const API_BASE_URL_VAR: &str = "CF_BASE_URL";
const API_BASE_URL_ENTRY: &str = "cloudflare.api.base_url";
const API_TOKEN_VAR: &str = "CF_TOKEN";
const API_TOKEN_ENTRY: &str = "cloudflare.api.token";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse {API_BASE_URL_ENTRY:?}: {0}
    ParseApiBaseUrl(provider::Error),
    /// Failed to parse {API_TOKEN_ENTRY:?}: {0}
    ParseApiToken(provider::Error),
    /// Failed to parse {API_ZONE_ID_ENTRY:?}: {0}
    ParseApiZoneId(provider::Error),
    /// Failed to parse {DNS_BASE_ENTRY:?}: {0}
    ParseDnsBase(provider::Error),
    /// Failed to parse {DNS_TTL_ENTRY:?}: {0}
    ParseDnsTtl(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub api: ApiConfig,
    pub dns: DnsConfig,
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(Config {
            api: provider.try_into()?,
            dns: provider.try_into()?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DnsConfig {
    pub base: String,
    pub ttl: i64,
}

impl TryFrom<&Provider> for DnsConfig {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(DnsConfig {
            base: provider
                .read(DNS_BASE_VAR, DNS_BASE_ENTRY)
                .map_err(Error::ParseDnsBase)?,
            ttl: provider
                .read(DNS_TTL_VAR, DNS_TTL_ENTRY)
                .map_err(Error::ParseDnsTtl)?,
        })
    }
}

#[derive(Debug, Deref, Deserialize, FromStr)]
#[deref(forward)]
pub struct ApiToken(Redacted<String>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiConfig {
    pub base_url: String,
    pub zone_id: String,
    pub token: ApiToken,
}

impl TryFrom<&Provider> for ApiConfig {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(ApiConfig {
            base_url: provider
                .read(API_BASE_URL_VAR, API_BASE_URL_ENTRY)
                .map_err(Error::ParseApiBaseUrl)?,
            zone_id: provider
                .read(API_ZONE_ID_VAR, API_ZONE_ID_ENTRY)
                .map_err(Error::ParseApiZoneId)?,
            token: provider
                .read(API_TOKEN_VAR, API_TOKEN_ENTRY)
                .map_err(Error::ParseApiToken)?,
        })
    }
}
