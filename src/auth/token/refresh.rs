use derive_more::{AsRef, Deref, Into};
use displaydoc::Display;
use jsonwebtoken::{errors, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tonic::metadata::AsciiMetadataValue;

use super::Expirable;
use crate::config::token::RefreshSecret;

const ALGORITHM: Algorithm = Algorithm::HS512;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to encode refresh token: {0}
    Encode(errors::Error),
    /// Failed to decode refresh token: {0}
    Decode(errors::Error),
    /// Refresh token `exp` is before `iat`. This should not happen.
    ExpiresBeforeIssued,
    /// Failed to create refresh cookie: {0}
    RefreshCookie(tonic::metadata::errors::InvalidMetadataValue),
}

#[derive(AsRef, Deref, Into)]
pub struct Encoded(String);

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

    pub fn decode(&self, raw: &str) -> Result<Refresh, Error> {
        let refresh: Refresh = jsonwebtoken::decode(raw, &self.decoding_key, &self.validation)
            .map(|data| data.claims)
            .map_err(Error::Decode)?;

        if refresh.exp < refresh.iat {
            return Err(Error::ExpiresBeforeIssued);
        }

        Ok(refresh)
    }

    pub fn cookie(&self, refresh: &Refresh) -> Result<AsciiMetadataValue, Error> {
        let expires = refresh.exp.format("%a, %d %b %Y %H:%M:%S GMT");
        let encoded = self.encode(refresh)?;

        let cookie = format!(
            "refresh={}; path=/; expires={expires}; Secure; HttpOnly; SameSite=None",
            encoded.as_ref()
        );

        cookie.parse().map_err(Error::RefreshCookie)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refresh {
    pub resource_id: uuid::Uuid,
    #[serde(with = "super::timestamp")]
    iat: chrono::DateTime<chrono::Utc>,
    #[serde(with = "super::timestamp")]
    pub exp: chrono::DateTime<chrono::Utc>,
}

impl Refresh {
    pub fn new(
        resource_id: uuid::Uuid,
        iat: chrono::DateTime<chrono::Utc>,
        exp: chrono::Duration,
    ) -> crate::Result<Self> {
        let expirable = Expirable::new(iat, exp)?;
        Ok(Self {
            resource_id,
            iat: expirable.iat(),
            exp: expirable.exp(),
        })
    }

    /// Returns the longevity of this token.
    pub fn duration(&self) -> chrono::Duration {
        self.exp - self.iat
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Context;

    #[test]
    fn test_encode_decode_preserves_token() {
        let context = Context::new_with_default_toml().unwrap();

        let refresh = Refresh::new(
            uuid::Uuid::new_v4(),
            chrono::Utc::now(),
            chrono::Duration::seconds(1),
        )
        .unwrap();

        let encoded = context.cipher.refresh.encode(&refresh).unwrap();
        let decoded = context.cipher.refresh.decode(&encoded).unwrap();
        assert_eq!(decoded, refresh);
    }
}
