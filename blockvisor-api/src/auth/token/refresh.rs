use std::str::FromStr;

use chrono::{DateTime, Utc};
use derive_more::{AsRef, Deref, From, Into};
use displaydoc::Display;
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tonic::metadata::AsciiMetadataValue;
use tracing::warn;

use crate::auth::claims::Expirable;
use crate::auth::resource::{ClaimsResource, Resource};
use crate::config::token::{RefreshSecret, RefreshSecrets};
use crate::grpc::{Metadata, Status};

const ALGORITHM: Algorithm = Algorithm::HS512;
const COOKIE_HEADER: &str = "cookie";
const COOKIE_REFRESH: &str = "refresh=";
const COOKIE_EXPIRES: &str = "expires=";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to decode refresh token: {0:?}
    Decode(ErrorKind),
    /// Failed to decode possibly expired refresh token: {0:?}
    DecodeExpired(ErrorKind),
    /// Failed to encode refresh token: {0:?}
    Encode(ErrorKind),
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
    ParseCookieHeader(hyper::header::ToStrError),
    /// Failed to create refresh cookie: {0}
    RefreshCookie(tonic::metadata::errors::InvalidMetadataValue),
    /// The refresh token for resource {0} has expired.
    TokenExpired(String),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            EmptyCookieRefresh | MissingCookieExpires => {
                Status::unauthorized("Invalid refresh cookie.")
            }
            MissingCookieHeader | MissingCookieRefresh => {
                Status::unauthorized("Missing refresh cookie.")
            }
            _ => Status::internal("Internal error."),
        }
    }
}

pub struct Cipher {
    header: Header,
    validation: Validation,
    validation_expired: Validation,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    fallback_decoding_keys: Vec<DecodingKey>,
}

impl Cipher {
    pub fn new(secret: &RefreshSecret, fallback_secrets: &RefreshSecrets) -> Self {
        let validation = Validation::new(ALGORITHM);
        let mut validation_expired = validation.clone();
        validation_expired.validate_exp = false;
        Cipher {
            header: Header::new(ALGORITHM),
            validation,
            validation_expired,
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
            .map_err(|err| Error::Encode(err.into_kind()))
    }

    pub fn decode(&self, encoded: &Encoded) -> Result<Refresh, Error> {
        let refresh: Refresh = jsonwebtoken::decode(encoded, &self.decoding_key, &self.validation)
            .map(|data| data.claims)
            .or_else(|err| {
                for key in &self.fallback_decoding_keys {
                    if let Ok(data) = jsonwebtoken::decode(encoded, key, &self.validation) {
                        return Ok(data.claims);
                    }
                }

                match err.into_kind() {
                    ErrorKind::ExpiredSignature => {
                        let refresh = self.decode_expired(encoded).ok();
                        let resource = refresh
                            .map_or_else(|| "unknown".to_string(), |r| format!("{:?}", r.resource));
                        Err(Error::TokenExpired(resource))
                    }
                    kind => Err(Error::Decode(kind)),
                }
            })?;

        if refresh.expirable.expires_at < refresh.expirable.issued_at {
            return Err(Error::ExpiresBeforeIssued);
        }

        Ok(refresh)
    }

    pub fn decode_expired(&self, encoded: &Encoded) -> Result<Refresh, Error> {
        let refresh: Refresh =
            jsonwebtoken::decode(encoded, &self.decoding_key, &self.validation_expired)
                .map(|data| data.claims)
                .or_else(|err| {
                    for key in &self.fallback_decoding_keys {
                        if let Ok(data) =
                            jsonwebtoken::decode(encoded, key, &self.validation_expired)
                        {
                            return Ok(data.claims);
                        }
                    }

                    Err(Error::DecodeExpired(err.into_kind()))
                })?;

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
    #[serde(flatten)]
    resource: ClaimsResource,
    #[serde(flatten)]
    expirable: Expirable,
}

impl Refresh {
    pub fn from_now<R: Into<Resource>>(expires: chrono::Duration, resource: R) -> Self {
        Refresh {
            resource: ClaimsResource::from(resource.into()),
            expirable: Expirable::from_now(expires),
        }
    }

    pub fn resource(&self) -> Resource {
        Resource::new(self.resource.resource_type, self.resource.resource_id)
    }

    pub const fn expirable(&self) -> Expirable {
        self.expirable
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

impl TryFrom<&Metadata> for RequestCookie {
    type Error = Error;

    fn try_from(meta: &Metadata) -> Result<Self, Self::Error> {
        meta.get_http(COOKIE_HEADER)
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
    use axum::http::HeaderValue;
    use uuid::Uuid;

    use crate::config::Context;
    use crate::grpc::Metadata;
    use crate::util::SecondsUtc;

    use super::*;

    const fn seconds(n: i64) -> chrono::Duration {
        chrono::Duration::seconds(n)
    }

    #[tokio::test]
    async fn test_refresh_encode_decode() {
        let ctx = Context::from_default_toml().await.unwrap();
        let refresh = Refresh::from_now(seconds(60), Resource::User(Uuid::new_v4().into()));

        let encoded = ctx.auth.cipher.refresh.encode(&refresh).unwrap();
        let decoded = ctx.auth.cipher.refresh.decode(&encoded).unwrap();
        assert_eq!(decoded, refresh);
    }

    #[tokio::test]
    async fn test_empty_refresh() {
        let ctx = Context::from_default_toml().await.unwrap();

        let mut meta = Metadata::new();
        meta.insert_http(COOKIE_HEADER, ";refresh=".parse::<HeaderValue>().unwrap());
        assert!(ctx.auth.refresh(&meta).is_err());

        let mut meta = Metadata::new();
        meta.insert_http(COOKIE_HEADER, "refresh=;".parse::<HeaderValue>().unwrap());
        assert!(ctx.auth.refresh(&meta).is_err());
    }

    #[tokio::test]
    async fn test_refresh_cookie() {
        let ctx = Context::from_default_toml().await.unwrap();
        let refresh = Refresh::from_now(seconds(60), Resource::Host(Uuid::new_v4().into()));

        let mut meta = Metadata::new();
        let cookie = ctx.auth.cipher.refresh.cookie(&refresh).unwrap();
        meta.insert_grpc(COOKIE_HEADER, cookie.header().unwrap());

        let result = ctx.auth.refresh(&meta).unwrap();
        assert_eq!(result.resource.resource_id, refresh.resource().id());
    }

    #[tokio::test]
    async fn test_extra_cookies() {
        let (ctx, db) = Context::with_mocked().await.unwrap();

        let user_id = db.seed.member.id;
        let refresh = Refresh::from_now(seconds(60), user_id);
        let encoded = ctx.auth.cipher.refresh.encode(&refresh).unwrap();

        let mut meta = Metadata::new();
        meta.insert_http(
            COOKIE_HEADER,
            format!("other_meta=v1; refresh={}; another=v2; ", *encoded)
                .parse::<HeaderValue>()
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
