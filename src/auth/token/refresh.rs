use crate::auth::key_provider;
use jsonwebtoken as jwt;

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        // Note that we must uphold the invariant that exp > iat here.
        let exp = iat + exp;
        if exp < iat {
            return Err(crate::Error::unexpected(
                "api is misconfigured, exp is negative",
            ));
        }
        Ok(Self {
            resource_id,
            iat,
            exp,
        })
    }

    pub fn encode(&self) -> crate::Result<String> {
        let header = jwt::Header::new(jwt::Algorithm::HS512);
        let encoded = jwt::encode(&header, self, &Self::ekey()?)?;
        Ok(encoded)
    }

    pub fn decode(raw: &str) -> crate::Result<Self> {
        let validation = jwt::Validation::new(jwt::Algorithm::HS512);
        let decoded: Self = jwt::decode(raw, &Self::dkey()?, &validation)?.claims;
        // Note that we must uphold the invariant that exp > iat here.
        if decoded.exp < decoded.iat {
            return Err(crate::Error::unexpected(
                "api is misconfigured, exp is negative",
            ));
        }
        Ok(decoded)
    }

    pub fn as_set_cookie(&self) -> crate::Result<String> {
        let exp = self.exp.format("%a, %d %b %Y %H:%M:%S GMT");
        let tkn = self.encode()?;
        let val = format!("refresh={tkn}; path=/; expires={exp}; secure; HttpOnly; SameSite=Lax");
        Ok(val)
    }

    /// Returns the longevity of this token.
    pub fn duration(&self) -> chrono::Duration {
        self.exp - self.iat
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
        let refresh = Refresh {
            resource_id: uuid::Uuid::new_v4(),
            iat,
            exp: iat + chrono::Duration::seconds(1),
        };
        let encoded = refresh.encode().unwrap();
        let decoded = Refresh::decode(&encoded).unwrap();
        assert_eq!(refresh, decoded);
    }
}
