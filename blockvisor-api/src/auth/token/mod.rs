pub mod api_key;
pub mod jwt;
pub mod refresh;

use std::str::FromStr;

use derive_more::{Deref, From};
use displaydoc::Display;
use thiserror::Error;
use tonic::{metadata::MetadataMap, Status};

use crate::config::token::SecretConfig;

use self::api_key::{KeyId, Secret};

const AUTH_HEADER: &str = "authorization";
const AUTH_HEADER_PREFIX: &str = "Bearer ";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Request header `{AUTH_HEADER:?}` must start with `{AUTH_HEADER_PREFIX:?}`.
    AuthHeaderPrefix,
    /// Missing `{AUTH_HEADER:?}` request header.
    MissingAuthHeader,
    /// Failed to parse `{AUTH_HEADER:?}` as string: {0}
    ParseAuthHeader(tonic::metadata::errors::ToStrError),
    /// Failed to parse KeyId: {0}
    ParseKeyId(api_key::Error),
    /// Failed to parse Secret: {0}
    ParseSecret(api_key::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            AuthHeaderPrefix | ParseAuthHeader(_) | ParseKeyId(_) | ParseSecret(_) => {
                Status::unauthenticated("Bad auth header.")
            }
            MissingAuthHeader => Status::unauthenticated("Missing auth header."),
        }
    }
}

pub struct Cipher {
    pub jwt: jwt::Cipher,
    pub refresh: refresh::Cipher,
}

impl Cipher {
    pub fn new(config: &SecretConfig) -> Self {
        Cipher {
            jwt: jwt::Cipher::new(&config.jwt, &config.jwt_fallback),
            refresh: refresh::Cipher::new(&config.refresh, &config.refresh_fallback),
        }
    }
}

/// An unverified request token from the `authorization` header.
pub enum RequestToken {
    ApiKey(ApiToken),
    Bearer(BearerToken),
}

impl TryFrom<&MetadataMap> for RequestToken {
    type Error = Error;

    fn try_from(meta: &MetadataMap) -> Result<Self, Self::Error> {
        meta.get(AUTH_HEADER)
            .ok_or(Error::MissingAuthHeader)?
            .to_str()
            .map_err(Error::ParseAuthHeader)?
            .strip_prefix(AUTH_HEADER_PREFIX)
            .ok_or(Error::AuthHeaderPrefix)?
            .parse()
    }
}

impl FromStr for RequestToken {
    type Err = Error;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        if !token.starts_with(api_key::TOKEN_PREFIX) {
            return Ok(RequestToken::Bearer(token.to_string().into()));
        }

        let key_id = KeyId::from_token(token).map_err(Error::ParseKeyId)?;
        let secret = Secret::from_token(token).map_err(Error::ParseSecret)?;

        Ok(RequestToken::ApiKey(ApiToken { key_id, secret }))
    }
}

/// An unverified API key token.
pub struct ApiToken {
    pub key_id: KeyId,
    pub secret: Secret,
}

/// An unverified bearer token.
#[derive(Deref, From)]
pub struct BearerToken(String);
