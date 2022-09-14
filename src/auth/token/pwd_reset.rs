use super::JwtToken;
use jsonwebtoken as jwt;
use std::str::FromStr;
use std::{env, str};

/// The claims of the token to be stored (encrypted) on the client side.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PwdResetToken {
    id: uuid::Uuid,
    exp: i64,
    holder_type: super::TokenHolderType,
    token_type: super::TokenType,
}

impl JwtToken for PwdResetToken {
    fn new(id: uuid::Uuid, exp: i64, holder_type: super::TokenHolderType) -> Self {
        Self {
            id,
            exp,
            holder_type,
            token_type: super::TokenType::PwdReset,
        }
    }

    fn token_holder(&self) -> super::TokenHolderType {
        self.holder_type
    }

    /// Get PWD_RESET_SECRET from env vars.
    fn get_secret() -> crate::auth::TokenResult<String> {
        match env::var("PWD_RESET_SECRET") {
            Ok(s) if s.is_empty() => panic!("`PWD_RESET_SECRET` parameter is empty"),
            Ok(secret) => Ok(secret),
            Err(e) => Err(super::TokenError::EnvVar(e)),
        }
    }
}

impl FromStr for PwdResetToken {
    type Err = super::TokenError;

    fn from_str(encoded: &str) -> Result<Self, Self::Err> {
        let secret = Self::get_secret()?;
        let validation = jwt::Validation::new(jwt::Algorithm::HS512);
        let key = jwt::DecodingKey::from_secret(secret.as_bytes());

        match jwt::decode(encoded, &key, &validation) {
            Ok(token) => Ok(token.claims),
            Err(e) => Err(super::TokenError::EnDeCoding(e)),
        }
    }
}

impl super::Identifier for PwdResetToken {
    fn get_id(&self) -> uuid::Uuid {
        self.id
    }
}
