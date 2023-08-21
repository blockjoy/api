use derive_more::{AsRef, Deref, Into};
use displaydoc::Display;
use jsonwebtoken::{errors, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use thiserror::Error;

use crate::auth::claims::Claims;
use crate::config::token::JwtSecret;

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

    pub fn encode(&self, claims: &Claims) -> Result<Jwt, Error> {
        jsonwebtoken::encode(&self.header, claims, &self.encoding_key)
            .map(Jwt)
            .map_err(Error::Encode)
    }

    pub fn decode(&self, token: &BearerToken) -> Result<Claims, Error> {
        jsonwebtoken::decode(token, &self.decoding_key, &self.validation)
            .map(|data| data.claims)
            .map_err(Error::Decode)
    }

    pub fn decode_expired(&self, token: &BearerToken) -> Result<Claims, Error> {
        let mut validation = Validation::new(ALGORITHM);
        validation.validate_exp = false;

        jsonwebtoken::decode(token, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(Error::DecodeExpired)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::auth::claims::Expirable;
    use crate::auth::endpoint::Endpoints;
    use crate::auth::resource::ResourceEntry;
    use crate::auth::token::RequestToken;
    use crate::config::Context;

    use super::*;

    #[tokio::test]
    async fn test_encode_decode_preserves_token() {
        let ctx = Context::from_default_toml().await.unwrap();

        let resource = ResourceEntry::new_node(Uuid::new_v4().into()).into();
        let expirable = Expirable::from_now(chrono::Duration::minutes(15));
        let claims = Claims::new(resource, expirable, Endpoints::Wildcard);

        let encoded = ctx.auth.cipher.jwt.encode(&claims).unwrap();
        let token = match encoded.parse().unwrap() {
            RequestToken::Bearer(token) => token,
            _ => panic!("Unexpected RequestToken type"),
        };

        let decoded = ctx.auth.cipher.jwt.decode(&token).unwrap();
        assert_eq!(claims, decoded);
    }
}
