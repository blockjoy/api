use derive_more::{AsRef, Deref, Into};
use displaydoc::Display;
use jsonwebtoken::{errors, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use thiserror::Error;

use crate::auth::Claims;
use crate::config::token::JwtSecret;

const ALGORITHM: Algorithm = Algorithm::HS512;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to encode JWT: {0}
    Encode(errors::Error),
    /// Failed to decode JWT: {0}
    Decode(errors::Error),
    /// Failed to decode expired JWT: {0}
    DecodeExpired(errors::Error),
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
    pub fn new(secret: &JwtSecret) -> Self {
        Cipher {
            header: Header::new(ALGORITHM),
            validation: Validation::new(ALGORITHM),
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    pub fn encode(&self, claims: &Claims) -> Result<Encoded, Error> {
        jsonwebtoken::encode(&self.header, claims, &self.encoding_key)
            .map(Encoded)
            .map_err(Error::Encode)
    }

    pub fn decode(&self, raw: &str) -> Result<Claims, Error> {
        jsonwebtoken::decode(raw, &self.decoding_key, &self.validation)
            .map(|data| data.claims)
            .map_err(Error::Decode)
    }

    pub fn decode_expired(&self, raw: &str) -> Result<Claims, Error> {
        let mut validation = Validation::new(ALGORITHM);
        validation.validate_exp = false;

        jsonwebtoken::decode(raw, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(Error::DecodeExpired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::token::{Endpoints, ResourceType};
    use crate::config::Context;

    #[test]
    fn test_encode_decode_preserves_token() {
        let context = Context::new_with_default_toml().unwrap();

        let iat = chrono::Utc::now();
        let claims = Claims::new(
            ResourceType::Node,
            uuid::Uuid::new_v4(),
            iat,
            chrono::Duration::minutes(15),
            Endpoints::Wildcard,
        )
        .unwrap();

        let encoded = context.cipher.jwt.encode(&claims).unwrap();
        let decoded = context.cipher.jwt.decode(&encoded).unwrap();
        assert_eq!(claims, decoded);
    }
}
