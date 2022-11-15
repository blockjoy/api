use crate::auth::{JwtToken, TokenClaim, TokenResult, TokenRole, TokenType};
use crate::errors::Result;
use derive_getters::Getters;
use std::str;
use std::str::FromStr;
use uuid::Uuid;

/// The claims of the token to be stored (encrypted) on the client side
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Getters)]
pub struct UserAuthToken {
    id: Uuid,
    exp: i64,
    token_type: TokenType,
    role: TokenRole,
}

#[tonic::async_trait]
impl JwtToken for UserAuthToken {
    fn get_expiration(&self) -> i64 {
        self.exp
    }

    fn get_id(&self) -> Uuid {
        self.id
    }

    fn try_new(claim: TokenClaim) -> TokenResult<Self> {
        Ok(Self {
            id: claim.id,
            exp: claim.exp,
            token_type: TokenType::UserAuth,
            role: claim.role,
        })
    }

    fn token_type(&self) -> TokenType {
        self.token_type
    }
}

impl FromStr for UserAuthToken {
    type Err = super::TokenError;

    fn from_str(encoded: &str) -> Result<Self, Self::Err> {
        UserAuthToken::from_encoded(encoded, TokenType::UserAuth, false)
    }
}

#[cfg(test)]
mod tests {
    use super::TokenClaim;
    use super::UserAuthToken;
    use crate::auth::{JwtToken, TokenRole, TokenType};
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn returns_true_for_expired_token() -> anyhow::Result<()> {
        let id = Uuid::new_v4();
        let exp = (Utc::now().timestamp() - 60000_i64) as i64;
        let claim = TokenClaim::new(id, exp, TokenType::UserAuth, TokenRole::User, None);
        let token = UserAuthToken::try_new(claim)?;

        Ok(assert!(token.has_expired()))
    }

    #[test]
    fn returns_false_for_not_expired_token() -> anyhow::Result<()> {
        let id = Uuid::new_v4();
        let exp = Utc::now().timestamp() + 60000;
        let claim = TokenClaim::new(id, exp, TokenType::UserAuth, TokenRole::User, None);
        let token = UserAuthToken::try_new(claim)?;

        Ok(assert!(!token.has_expired()))
    }
}
