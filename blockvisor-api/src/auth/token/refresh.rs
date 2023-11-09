use std::str::FromStr;

use chrono::{DateTime, Utc};
use derive_more::{AsRef, Deref, From, Into};
use displaydoc::Display;
use jsonwebtoken::{errors, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tonic::metadata::{AsciiMetadataValue, MetadataMap};
use tonic::Status;
use tracing::warn;

use crate::auth::claims::Expirable;
use crate::auth::resource::ResourceId;
use crate::config::token::{RefreshSecret, RefreshSecrets};

const ALGORITHM: Algorithm = Algorithm::HS512;
const COOKIE_HEADER: &str = "cookie";
const COOKIE_REFRESH: &str = "refresh=";
const COOKIE_EXPIRES: &str = "expires=";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to decode refresh token: {0}
    Decode(errors::Error),
    /// Failed to encode refresh token: {0}
    Encode(errors::Error),
    /// Empty `{COOKIE_REFRESH:?}` value in `{COOKIE_HEADER:?}`.
    EmptyCookieRefresh,
    /// Refresh token `exp` is before `iat`. This should not happen.
    ExpiresBeforeIssued,
    /// Missing `{COOKIE_HEADER:?}` request header.
    MissingCookieHeader,
    /// Missing `{COOKIE_EXPIRES:?}` in `{COOKIE_HEADER:?}`.
    MissingCookieExpires,
    /// Missing `{COOKIE_REFRESH:?}` in `{COOKIE_HEADER:?}`.
    MissingCookieRefresh,
    /// Failed to parse `{COOKIE_HEADER:?}` as string: {0}
    ParseCookieHeader(tonic::metadata::errors::ToStrError),
    /// Failed to create refresh cookie: {0}
    RefreshCookie(tonic::metadata::errors::InvalidMetadataValue),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            EmptyCookieRefresh | MissingCookieHeader | MissingCookieExpires
            | MissingCookieRefresh => Status::unauthenticated("Refresh cookie."),
            _ => Status::internal("Internal error."),
        }
    }
}

pub struct Cipher {
    header: Header,
    validation: Validation,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    fallback_decoding_keys: Vec<DecodingKey>,
}

impl Cipher {
    pub fn new(secret: &RefreshSecret, fallback_secrets: &RefreshSecrets) -> Self {
        Cipher {
            header: Header::new(ALGORITHM),
            validation: Validation::new(ALGORITHM),
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            fallback_decoding_keys: fallback_secrets
                .iter()
                .map(String::as_bytes)
                .map(DecodingKey::from_secret)
                .collect(),
        }
    }

    pub fn encode(&self, refresh: &Refresh) -> Result<Encoded, Error> {
        jsonwebtoken::encode(&self.header, refresh, &self.encoding_key)
            .map(Encoded)
            .map_err(Error::Encode)
    }

    pub fn decode(&self, encoded: &Encoded) -> Result<Refresh, Error> {
        let refresh = Self::decode_inner(
            encoded,
            &self.decoding_key,
            &self.fallback_decoding_keys,
            &self.validation,
        )
        .map_err(Error::Decode)?;

        if refresh.expirable.expires_at < refresh.expirable.issued_at {
            return Err(Error::ExpiresBeforeIssued);
        }

        Ok(refresh)
    }

    fn decode_inner(
        token: &Encoded,
        decoding_key: &DecodingKey,
        fallback_decoding_keys: &[DecodingKey],
        validation: &Validation,
    ) -> Result<Refresh, jsonwebtoken::errors::Error> {
        let err = match jsonwebtoken::decode(token, decoding_key, validation) {
            Ok(data) => return Ok(data.claims),
            Err(err) => err,
        };
        for decoding_key in fallback_decoding_keys {
            if let Ok(data) = jsonwebtoken::decode(token, decoding_key, validation) {
                return Ok(data.claims);
            }
        }
        Err(err)
    }

    pub fn cookie(&self, refresh: &Refresh) -> Result<RequestCookie, Error> {
        RequestCookie::new(refresh, self)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refresh {
    resource_id: ResourceId,
    #[serde(flatten)]
    expirable: Expirable,
}

impl Refresh {
    pub const fn new(resource_id: ResourceId, expirable: Expirable) -> Self {
        Self {
            resource_id,
            expirable,
        }
    }

    pub fn from_now<R: Into<ResourceId>>(expires: chrono::Duration, resource_id: R) -> Self {
        Refresh {
            resource_id: resource_id.into(),
            expirable: Expirable::from_now(expires),
        }
    }

    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    pub const fn expirable(&self) -> Expirable {
        self.expirable
    }

    pub fn cookie(&self, cipher: &Cipher) -> Result<RequestCookie, Error> {
        RequestCookie::new(self, cipher)
    }
}

/// An encoded representation of the `Refresh` token.
#[derive(AsRef, Deref, From, Into)]
pub struct Encoded(String);

pub struct RequestCookie {
    pub encoded: Encoded,
    pub expires: Option<DateTime<Utc>>,
}

impl RequestCookie {
    pub fn new(refresh: &Refresh, cipher: &Cipher) -> Result<Self, Error> {
        let encoded = cipher.encode(refresh)?;
        let expires = Some(refresh.expirable.expires_at.into());

        Ok(RequestCookie { encoded, expires })
    }

    pub fn header(&self) -> Result<AsciiMetadataValue, Error> {
        let cookie = format!(
            "refresh={}; path=/; expires={}; Secure; HttpOnly; SameSite=None",
            self.encoded.as_ref(),
            self.expires
                .ok_or(Error::MissingCookieExpires)?
                .to_rfc2822()
        );

        cookie.parse().map_err(Error::RefreshCookie)
    }
}

impl TryFrom<&MetadataMap> for RequestCookie {
    type Error = Error;

    fn try_from(meta: &MetadataMap) -> Result<Self, Self::Error> {
        meta.get(COOKIE_HEADER)
            .ok_or(Error::MissingCookieHeader)?
            .to_str()
            .map_err(Error::ParseCookieHeader)?
            .parse()
    }
}

impl FromStr for RequestCookie {
    type Err = Error;

    fn from_str(cookie: &str) -> Result<Self, Self::Err> {
        let encoded = {
            let start = cookie
                .find(COOKIE_REFRESH)
                .map(|index| index + COOKIE_REFRESH.len())
                .ok_or(Error::MissingCookieRefresh)?;

            let end = match cookie[start..].find(';') {
                Some(index) => Ok(start + index),
                None if start + 1 >= cookie.len() => Err(Error::EmptyCookieRefresh),
                None => Ok(cookie.len()),
            }?;

            Ok(Encoded(cookie[start..end].to_string()))
        }?;

        let expires = 'option: {
            let start = match cookie.find(COOKIE_EXPIRES) {
                Some(index) => index + COOKIE_EXPIRES.len(),
                None => break 'option None,
            };

            let end = match cookie[start..].find(';') {
                Some(index) => start + index,
                None if start + 1 >= cookie.len() => {
                    warn!("Cookie `expires` value is empty");
                    break 'option None;
                }
                None => cookie.len(),
            };

            match DateTime::parse_from_rfc2822(&cookie[start..end]) {
                Ok(datetime) => Some(datetime.into()),
                Err(err) => {
                    warn!("Failed to parse cookie `expires` value: {err}");
                    None
                }
            }
        };

        Ok(RequestCookie { encoded, expires })
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::config::Context;
    use crate::util::SecondsUtc;

    use super::*;

    fn seconds(n: i64) -> chrono::Duration {
        chrono::Duration::seconds(n)
    }

    #[tokio::test]
    async fn test_refresh_encode_decode() {
        let ctx = Context::from_default_toml().await.unwrap();
        let refresh = Refresh::from_now(seconds(60), Uuid::new_v4());

        let encoded = ctx.auth.cipher.refresh.encode(&refresh).unwrap();
        let decoded = ctx.auth.cipher.refresh.decode(&encoded).unwrap();
        assert_eq!(decoded, refresh);
    }

    #[tokio::test]
    async fn test_empty_refresh() {
        let ctx = Context::from_default_toml().await.unwrap();

        let mut meta = MetadataMap::new();
        meta.insert(COOKIE_HEADER, ";refresh=".parse().unwrap());
        assert!(ctx.auth.refresh(&meta).is_err());

        let mut meta = MetadataMap::new();
        meta.insert(COOKIE_HEADER, "refresh=;".parse().unwrap());
        assert!(ctx.auth.refresh(&meta).is_err());
    }

    #[tokio::test]
    async fn test_refresh_cookie() {
        let ctx = Context::from_default_toml().await.unwrap();
        let refresh = Refresh::from_now(seconds(60), Uuid::new_v4());

        let mut meta = MetadataMap::new();
        let cookie = ctx.auth.cipher.refresh.cookie(&refresh).unwrap();
        meta.insert(COOKIE_HEADER, cookie.header().unwrap());

        let result = ctx.auth.refresh(&meta).unwrap();
        assert_eq!(result.resource_id, refresh.resource_id);
    }

    #[tokio::test]
    async fn test_extra_cookies() {
        let (ctx, db) = Context::with_mocked().await.unwrap();

        let user_id = db.seed.user.id;
        let refresh = Refresh::from_now(seconds(60), user_id);
        let encoded = ctx.auth.cipher.refresh.encode(&refresh).unwrap();

        let mut meta = MetadataMap::new();
        meta.insert(
            COOKIE_HEADER,
            format!("other_meta=v1; refresh={}; another=v2; ", *encoded)
                .parse()
                .unwrap(),
        );
        ctx.auth.refresh(&meta).unwrap();
    }

    #[tokio::test]
    async fn test_allow_missing_expires() {
        let encoded = "someval".to_string();
        let tests = [
            format!("refresh={encoded}"),
            format!("refresh={encoded};"),
            format!("refresh={encoded}; expires="),
            format!("refresh={encoded}; expires=;"),
        ];

        for test in tests {
            let cookie: RequestCookie = test.parse().unwrap();
            assert_eq!(*cookie.encoded, encoded);
            assert!(cookie.expires.is_none());
        }

        let expires = SecondsUtc::now();
        let test = format!("refresh={encoded}; expires={}", expires.to_rfc2822());

        let cookie: RequestCookie = test.parse().unwrap();
        assert_eq!(*cookie.encoded, encoded);
        assert_eq!(cookie.expires.unwrap(), *expires);
    }
}
