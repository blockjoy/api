use super::JwtToken;
use jsonwebtoken as jwt;
use std::str::FromStr;
use std::{env, str};

/// The claims of the token to be stored (encrypted) on the client side
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct AuthToken {
    id: uuid::Uuid,
    exp: i64,
    holder_type: super::TokenHolderType,
}

impl JwtToken for AuthToken {
    fn new(id: uuid::Uuid, exp: i64, holder_type: super::TokenHolderType) -> Self {
        Self {
            id,
            exp,
            holder_type,
        }
    }

    fn token_holder(&self) -> super::TokenHolderType {
        self.holder_type
    }

    fn get_secret() -> crate::auth::TokenResult<String> {
        match env::var("JWT_SECRET") {
            Ok(s) if s.is_empty() => panic!("`JWT_SECRET` parameter is empty"),
            Ok(secret) => Ok(secret),
            Err(e) => Err(super::TokenError::EnvVar(e)),
        }
    }
}

impl FromStr for AuthToken {
    type Err = super::TokenError;

    fn from_str(encoded: &str) -> Result<Self, Self::Err> {
        let secret = Self::get_secret()?;
        let mut validation = jwt::Validation::new(jwt::Algorithm::HS512);

        validation.validate_exp = true;

        match jwt::decode::<AuthToken>(
            encoded,
            &jwt::DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        ) {
            Ok(token) => Ok(token.claims),
            Err(e) => Err(super::TokenError::EnDeCoding(e)),
        }
    }
}

impl super::Identifier for AuthToken {
    fn get_id(&self) -> uuid::Uuid {
        self.id
    }
}
