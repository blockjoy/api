use crate::auth::{Blacklisted, JwtToken, TokenClaim, TokenResult, TokenType};
use crate::errors::Result;
use crate::models::BlacklistToken;
use crate::server::DbPool;
use std::str;
use std::str::FromStr;
use uuid::Uuid;

/// The claims of the token to be stored (encrypted) on the client side.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PwdResetToken {
    id: Uuid,
    exp: i64,
    token_type: TokenType,
}

impl JwtToken for PwdResetToken {
    fn get_expiration(&self) -> i64 {
        self.exp
    }

    fn get_id(&self) -> Uuid {
        self.id
    }

    fn new(claim: TokenClaim) -> Self {
        Self {
            id: claim.id,
            exp: claim.exp,
            token_type: TokenType::PwdReset,
        }
    }

    fn token_type(&self) -> TokenType {
        self.token_type
    }
}

#[tonic::async_trait]
impl Blacklisted for PwdResetToken {
    async fn blacklist(&self, db: DbPool) -> TokenResult<bool> {
        Ok(BlacklistToken::create(self.encode()?, self.token_type, &db)
            .await
            .is_ok())
    }

    async fn is_blacklisted(&self, token: String, db: DbPool) -> TokenResult<bool> {
        Ok(BlacklistToken::is_listed(token, &db).await.is_ok())
    }
}

impl FromStr for PwdResetToken {
    type Err = super::TokenError;

    fn from_str(encoded: &str) -> Result<Self, Self::Err> {
        PwdResetToken::from_encoded::<PwdResetToken>(encoded, TokenType::PwdReset)
    }
}
