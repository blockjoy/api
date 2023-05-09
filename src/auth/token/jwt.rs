use crate::auth::{self, key_provider};
use jsonwebtoken as jwt;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Jwt {
    #[serde(flatten)]
    pub claims: auth::Claims,
}

impl Jwt {
    pub fn encode(&self) -> crate::Result<String> {
        let header = jwt::Header::new(jwt::Algorithm::HS512);
        let encoded = jwt::encode(&header, &self.claims, &Self::ekey()?)?;
        Ok(encoded)
    }

    pub fn decode(raw: &str) -> crate::Result<Self> {
        let validation = jwt::Validation::new(jwt::Algorithm::HS512);
        let decoded = jwt::decode(raw, &Self::dkey()?, &validation)?;
        Ok(Self {
            claims: decoded.claims,
        })
    }

    pub fn decode_expired(raw: &str) -> crate::Result<Self> {
        let mut validation = jwt::Validation::new(jwt::Algorithm::HS512);
        validation.validate_exp = false;
        let decoded = jwt::decode(raw, &Self::dkey()?, &validation)?;
        Ok(Self {
            claims: decoded.claims,
        })
    }

    fn dkey() -> crate::Result<jwt::DecodingKey> {
        let key = key_provider::KeyProvider::jwt_secret()?;
        Ok(jwt::DecodingKey::from_secret(key.as_bytes()))
    }

    fn ekey() -> crate::Result<jwt::EncodingKey> {
        let key = key_provider::KeyProvider::jwt_secret()?;
        Ok(jwt::EncodingKey::from_secret(key.as_bytes()))
    }
}
