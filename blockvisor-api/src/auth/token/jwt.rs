use derive_more::{AsRef, Deref, Into};
use displaydoc::Display;
use jsonwebtoken::{errors, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use thiserror::Error;

use crate::auth::claims::Claims;
use crate::config::token::{JwtSecret, JwtSecrets};

use super::BearerToken;

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

/// An encoded representation of a JWT for authentication.
#[derive(AsRef, Deref, Into)]
pub struct Jwt(String);

pub struct Cipher {
    header: Header,
    validation: Validation,
    validation_expired_tokens: Validation,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    fallback_decoding_keys: Vec<DecodingKey>,
}

impl Cipher {
    pub fn new(secret: &JwtSecret, fallback_secrets: &JwtSecrets) -> Self {
        let validation = Validation::new(ALGORITHM);
        let mut validation_expired_tokens = validation.clone();
        validation_expired_tokens.validate_exp = false;
        Cipher {
            header: Header::new(ALGORITHM),
            validation,
            validation_expired_tokens,
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            fallback_decoding_keys: fallback_secrets
                .iter()
                .map(String::as_bytes)
                .map(DecodingKey::from_secret)
                .collect(),
        }
    }

    pub fn encode(&self, claims: &Claims) -> Result<Jwt, Error> {
        jsonwebtoken::encode(&self.header, claims, &self.encoding_key)
            .map(Jwt)
            .map_err(Error::Encode)
    }

    pub fn decode(&self, token: &BearerToken) -> Result<Claims, Error> {
        Self::decode_inner(
            token,
            &self.decoding_key,
            &self.fallback_decoding_keys,
            &self.validation,
        )
        .map_err(Error::Decode)
    }

    pub fn decode_expired(&self, token: &BearerToken) -> Result<Claims, Error> {
        Self::decode_inner(
            token,
            &self.decoding_key,
            &self.fallback_decoding_keys,
            &self.validation_expired_tokens,
        )
        .map_err(Error::DecodeExpired)
    }

    fn decode_inner(
        token: &BearerToken,
        decoding_key: &DecodingKey,
        fallback_decoding_keys: &[DecodingKey],
        validation: &Validation,
    ) -> Result<Claims, jsonwebtoken::errors::Error> {
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
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::auth::claims::tests::claims_none;
    use crate::auth::token::RequestToken;
    use crate::config::Context;

    #[tokio::test]
    async fn test_encode_decode_preserves_token() {
        let ctx = Context::from_default_toml().await.unwrap();

        let claims = claims_none(Uuid::new_v4().into());
        let encoded = ctx.auth.cipher.jwt.encode(&claims).unwrap();
        let RequestToken::Bearer(token) = encoded.parse().unwrap() else {
            panic!("Unexpected RequestToken type")
        };

        let decoded = ctx.auth.cipher.jwt.decode(&token).unwrap();
        assert_eq!(claims, decoded);
    }
}
