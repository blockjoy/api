use std::str::FromStr;

use chrono::{DateTime, Utc};
use derive_more::{AsRef, Deref, From, Into};
use displaydoc::Display;
use jsonwebtoken::{errors, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tonic::metadata::{AsciiMetadataValue, MetadataMap};
use tracing::warn;

use crate::auth::claims::Expirable;
use crate::auth::resource::ResourceId;
use crate::config::token::RefreshSecret;

const ALGORITHM: Algorithm = Algorithm::HS512;
const COOKIE_HEADER: &str = "cookie";
const COOKIE_REFRESH: &str = "refresh=";
const COOKIE_EXPIRES: &str = "expires=";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to encode refresh token: {0}
    Encode(errors::Error),
    /// Failed to decode refresh token: {0}
    Decode(errors::Error),
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

pub struct Cipher {
    header: Header,
    validation: Validation,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl Cipher {
    pub fn new(secret: &RefreshSecret) -> Self {
        Cipher {
            header: Header::new(ALGORITHM),
            validation: Validation::new(ALGORITHM),
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    pub fn encode(&self, refresh: &Refresh) -> Result<Encoded, Error> {
        jsonwebtoken::encode(&self.header, refresh, &self.encoding_key)
            .map(Encoded)
            .map_err(Error::Encode)
    }

    pub fn decode(&self, encoded: &Encoded) -> Result<Refresh, Error> {
        let refresh: Refresh = jsonwebtoken::decode(encoded, &self.decoding_key, &self.validation)
            .map(|data| data.claims)
            .map_err(Error::Decode)?;

        if refresh.expirable.expires_at < refresh.expirable.issued_at {
            return Err(Error::ExpiresBeforeIssued);
        }

        Ok(refresh)
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
    pub fn new(resource_id: ResourceId, expirable: Expirable) -> Self {
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

    pub fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    pub fn expirable(&self) -> Expirable {
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
            let start = match cookie.find(COOKIE_REFRESH) {
                Some(index) => Ok(index + COOKIE_REFRESH.len()),
                None => Err(Error::MissingCookieRefresh),
            }?;

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
    use crate::timestamp::SecondsUtc;

    use super::*;

    fn seconds(n: i64) -> chrono::Duration {
        chrono::Duration::seconds(n)
    }

    #[tokio::test]
    async fn test_refresh_encode_decode() {
        let context = Context::from_default_toml().await.unwrap();
        let refresh = Refresh::from_now(seconds(60), Uuid::new_v4());

        let encoded = context.cipher().refresh.encode(&refresh).unwrap();
        let decoded = context.cipher().refresh.decode(&encoded).unwrap();
        assert_eq!(decoded, refresh);
    }

    #[tokio::test]
    async fn test_empty_refresh() {
        let context = Context::from_default_toml().await.unwrap();

        let mut req = tonic::Request::new(());
        req.metadata_mut()
            .insert(COOKIE_HEADER, ";refresh=".parse().unwrap());
        assert!(context.auth.refresh(&req).is_err());

        let mut req = tonic::Request::new(());
        req.metadata_mut()
            .insert(COOKIE_HEADER, "refresh=;".parse().unwrap());
        assert!(context.auth.refresh(&req).is_err());
    }

    #[tokio::test]
    async fn test_refresh_cookie() {
        let context = Context::from_default_toml().await.unwrap();
        let refresh = Refresh::from_now(seconds(60), Uuid::new_v4());

        let mut req = tonic::Request::new(());
        let cookie = context.cipher().refresh.cookie(&refresh).unwrap();
        req.metadata_mut()
            .insert(COOKIE_HEADER, cookie.header().unwrap());

        let res = context.auth.refresh(&req).unwrap();
        assert_eq!(res.resource_id, refresh.resource_id);
    }

    #[tokio::test]
    async fn test_extra_cookies() {
        let (context, db) = Context::with_mocked().await.unwrap();

        let resource_id = db.user().await.id;
        let refresh = Refresh::from_now(seconds(60), resource_id);
        let encoded = context.cipher().refresh.encode(&refresh).unwrap();

        let mut req = tonic::Request::new(());
        req.metadata_mut().insert(
            COOKIE_HEADER,
            format!("other_meta=v1; refresh={}; another=v2; ", *encoded)
                .parse()
                .unwrap(),
        );
        context.auth.refresh(&req).unwrap();
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
