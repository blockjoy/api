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
        let claims = jwt::decode(raw, &Self::dkey()?, &validation)?.claims;
        Ok(Self { claims })
    }

    pub fn decode_expired(raw: &str) -> crate::Result<Self> {
        let mut validation = jwt::Validation::new(jwt::Algorithm::HS512);
        validation.validate_exp = false;
        let claims = jwt::decode(raw, &Self::dkey()?, &validation)?.claims;
        Ok(Self { claims })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_preserves_token() {
        let iat = chrono::Utc::now();
        let claims = auth::Claims::new(
            auth::ResourceType::Node,
            uuid::Uuid::new_v4(),
            iat,
            chrono::Duration::minutes(15),
            auth::Endpoints::Wildcard,
            Default::default(),
        )
        .unwrap();
        let token = Jwt { claims };
        let encoded = token.encode().unwrap();
        let decoded = Jwt::decode(&encoded).unwrap();
        assert_eq!(token.claims, decoded.claims);
    }
}
