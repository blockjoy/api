use derive_more::{AsRef, Deref, Into};
use displaydoc::Display;
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use thiserror::Error;

use crate::auth::claims::Claims;
use crate::config::token::{JwtSecret, JwtSecrets};

use super::BearerToken;

const ALGORITHM: Algorithm = Algorithm::HS512;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to encode JWT: {0:?}
    Encode(ErrorKind),
    /// Failed to decode JWT: {0:?}
    Decode(ErrorKind),
    /// Failed to decode expired JWT: {0:?}
    DecodeExpired(ErrorKind),
    /// The JWT has expired.
    TokenExpired,
}

/// An encoded representation of a JWT for authentication.
#[derive(AsRef, Deref, Into)]
pub struct Jwt(String);

pub struct Cipher {
    header: Header,
    validation: Validation,
    validation_expired: Validation,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    fallback_decoding_keys: Vec<DecodingKey>,
}

impl Cipher {
    pub fn new(secret: &JwtSecret, fallback_secrets: &JwtSecrets) -> Self {
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

    pub fn encode(&self, claims: &Claims) -> Result<Jwt, Error> {
        jsonwebtoken::encode(&self.header, claims, &self.encoding_key)
            .map(Jwt)
            .map_err(|err| Error::Encode(err.into_kind()))
    }

    pub fn decode(&self, token: &BearerToken) -> Result<Claims, Error> {
        jsonwebtoken::decode(token, &self.decoding_key, &self.validation)
            .map(|data| data.claims)
            .or_else(|err| {
                for key in &self.fallback_decoding_keys {
                    if let Ok(data) = jsonwebtoken::decode(token, key, &self.validation) {
                        return Ok(data.claims);
                    }
                }

                match err.into_kind() {
                    ErrorKind::ExpiredSignature => Err(Error::TokenExpired),
                    kind => Err(Error::Decode(kind)),
                }
            })
    }

    pub fn decode_expired(&self, token: &BearerToken) -> Result<Claims, Error> {
        jsonwebtoken::decode(token, &self.decoding_key, &self.validation_expired)
            .map(|data| data.claims)
            .or_else(|err| {
                for key in &self.fallback_decoding_keys {
                    if let Ok(data) = jsonwebtoken::decode(token, key, &self.validation_expired) {
                        return Ok(data.claims);
                    }
                }

                Err(Error::DecodeExpired(err.into_kind()))
            })
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use uuid::Uuid;

    use crate::auth::claims::{Claims, Expirable};
    use crate::auth::rbac::{Access, NodePerm, Perms};
    use crate::auth::resource::Resource;
    use crate::auth::token::RequestToken;
    use crate::config::Context;

    #[tokio::test]
    async fn test_encode_decode_preserves_token() {
        let ctx = Context::from_default_toml().await.unwrap();

        let expires = Duration::minutes(15);
        let claims = Claims {
            resource: Resource::User(Uuid::new_v4().into()).into(),
            expirable: Expirable::from_now(expires),
            access: Access::Perms(Perms::One(NodePerm::Create.into())),
            data: None,
        };

        let encoded = ctx.auth.cipher.jwt.encode(&claims).unwrap();
        let RequestToken::Bearer(token) = encoded.parse().unwrap() else {
            panic!("Unexpected RequestToken type")
        };

        let decoded = ctx.auth.cipher.jwt.decode(&token).unwrap();
        assert_eq!(claims, decoded);
    }
}
