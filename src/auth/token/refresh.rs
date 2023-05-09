use crate::auth::{expiration_provider, key_provider};
use jsonwebtoken as jwt;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Refresh {
    pub resource_id: uuid::Uuid,
    #[serde(with = "super::timestamp")]
    iat: chrono::DateTime<chrono::Utc>,
    #[serde(with = "super::timestamp")]
    pub exp: chrono::DateTime<chrono::Utc>,
}

impl Refresh {
    pub fn new(resource_id: uuid::Uuid, iat: chrono::DateTime<chrono::Utc>) -> crate::Result<Self> {
        let exp = expiration_provider::ExpirationProvider::expiration("TOKEN_EXPIRATION_MINS")?;
        Ok(Self {
            resource_id,
            iat,
            exp: iat + exp,
        })
    }

    pub fn encode(&self) -> crate::Result<String> {
        let header = jwt::Header::new(jwt::Algorithm::HS512);
        let encoded = jwt::encode(&header, self, &Self::ekey()?)?;
        Ok(encoded)
    }

    pub fn decode(raw: &str) -> crate::Result<Self> {
        let validation = jwt::Validation::new(jwt::Algorithm::HS512);
        let decoded = jwt::decode(raw, &Self::dkey()?, &validation)?;
        Ok(decoded.claims)
    }

    pub fn as_set_cookie(&self, iat: chrono::DateTime<chrono::Utc>) -> crate::Result<String> {
        let exp =
            expiration_provider::ExpirationProvider::expiration("REFRESH_TOKEN_EXPIRATION_MINS")?;
        let exp = (iat + exp).format("%a, %d %b %Y %H:%M:%S GMT");
        let tkn = self.encode()?;
        let val = format!("refresh={tkn}; path=/; expires={exp}; secure; HttpOnly; SameSite=Lax");
        Ok(val)
    }

    // pub fn resource(&self) -> Resource {
    //     match self.resource_type {
    //         ResourceType::User => Resource::User(self.resource_id),
    //         ResourceType::Org => Resource::Org(self.resource_id),
    //         ResourceType::Host => Resource::Host(self.resource_id),
    //         ResourceType::Node => Resource::Node(self.resource_id),
    //     }
    // }

    fn dkey() -> crate::Result<jwt::DecodingKey> {
        let key = key_provider::KeyProvider::jwt_secret()?;
        Ok(jwt::DecodingKey::from_secret(key.as_bytes()))
    }

    fn ekey() -> crate::Result<jwt::EncodingKey> {
        let key = key_provider::KeyProvider::jwt_secret()?;
        Ok(jwt::EncodingKey::from_secret(key.as_bytes()))
    }
}
