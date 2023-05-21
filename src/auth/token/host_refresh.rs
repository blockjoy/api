use crate::auth::{JwtToken, TokenClaim, TokenResult, TokenRole, TokenType};
use std::str::FromStr;
use uuid::Uuid;

/// The claims of the token to be stored (encrypted) on the client side.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct HostRefreshToken {
    id: Uuid,
    exp: i64,
    token_type: TokenType,
    role: TokenRole,
}

impl JwtToken for HostRefreshToken {
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
            token_type: TokenType::HostRefresh,
            role: claim.role,
        })
    }

    fn token_type(&self) -> TokenType {
        self.token_type
    }
}

impl FromStr for HostRefreshToken {
    type Err = super::TokenError;

    fn from_str(encoded: &str) -> Result<Self, Self::Err> {
        HostRefreshToken::from_encoded(encoded, TokenType::HostRefresh, true)
    }
}
