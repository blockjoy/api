use crate::auth::expiration_provider::ExpirationProvider;
use crate::auth::{JwtToken, TokenClaim, TokenError, TokenResult, TokenType};
use axum::http::Request as HttpRequest;
use derive_getters::Getters;
use std::str::FromStr;
use uuid::Uuid;

/// The claims of the token to be stored (encrypted) on the client side.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Getters)]
pub struct UserRefreshToken {
    id: Uuid,
    exp: i64,
    token_type: TokenType,
}

impl UserRefreshToken {
    pub fn create(id: Uuid) -> Self {
        Self {
            id,
            exp: ExpirationProvider::expiration(TokenType::UserRefresh),
            token_type: TokenType::UserRefresh,
        }
    }
}

impl JwtToken for UserRefreshToken {
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
            token_type: TokenType::UserRefresh,
        })
    }

    fn token_type(&self) -> TokenType {
        self.token_type
    }

    fn from_request<B>(request: &HttpRequest<B>) -> TokenResult<Self>
    where
        Self: FromStr<Err = TokenError>,
    {
        let val = request.headers().get("cookie");
        let val = val.ok_or(TokenError::Invalid)?;
        let val = val
            .to_str()
            .map_err(|_| TokenError::Invalid)?
            .split("refresh=")
            .nth(1)
            .ok_or(TokenError::Invalid)?;

        val.parse()
    }
}

impl FromStr for UserRefreshToken {
    type Err = TokenError;

    fn from_str(encoded: &str) -> Result<Self, Self::Err> {
        UserRefreshToken::from_encoded(encoded, TokenType::UserRefresh, true)
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::expiration_provider::ExpirationProvider;
    use crate::auth::{JwtToken, TokenClaim, TokenRole, TokenType, UserRefreshToken};
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn can_create_token() -> anyhow::Result<()> {
        temp_env::with_var_unset("SECRETS_ROOT", || {
            let mut role = HashMap::<String, String>::new();
            role.insert("role".to_string(), TokenRole::User.to_string());

            let claim = TokenClaim::new(
                Uuid::new_v4(),
                ExpirationProvider::expiration(TokenType::UserRefresh),
                TokenType::UserRefresh,
                TokenRole::User,
                None,
            );
            let encoded = UserRefreshToken::try_new(claim)?.encode()?;

            println!("Encoded token: {encoded:?}");
            assert!(encoded.starts_with("ey"));

            Ok(())
        })
    }

    #[test]
    fn can_decode_token() -> anyhow::Result<()> {
        temp_env::with_var_unset("SECRETS_ROOT", || {
            let user_id = Uuid::new_v4();
            let claim = TokenClaim::new(
                user_id,
                ExpirationProvider::expiration(TokenType::UserRefresh),
                TokenType::UserRefresh,
                TokenRole::User,
                None,
            );
            let encoded = UserRefreshToken::try_new(claim)?.encode()?;

            println!("Encoded token: {encoded:?}");
            assert!(encoded.starts_with("ey"));

            let token = UserRefreshToken::from_encoded::<UserRefreshToken>(
                encoded.as_str(),
                TokenType::UserRefresh,
                true,
            )
            .unwrap();

            assert_eq!(token.id, user_id);

            Ok(())
        })
    }
}
