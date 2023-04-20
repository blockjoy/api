use crate::auth::expiration_provider::ExpirationProvider;
use crate::auth::{
    Blacklisted, JwtToken, TokenClaim, TokenError, TokenResult, TokenRole, TokenType,
};
use crate::models::{self, Invitation};
use crate::Result;
use anyhow::anyhow;
use diesel_async::AsyncPgConnection;
use std::collections::HashMap;
use std::str;
use std::str::FromStr;
use uuid::Uuid;

/// The claims of the token to be stored (encrypted) on the client side.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct InvitationToken {
    pub id: Uuid,
    exp: i64,
    token_type: TokenType,
    pub role: TokenRole,
    email: String,
}

impl InvitationToken {
    pub fn create_for_invitation(invitation: &Invitation) -> TokenResult<Self> {
        let mut data: HashMap<String, String> = HashMap::new();
        let exp = ExpirationProvider::expiration(TokenType::Invitation);

        data.insert("invitee_email".into(), invitation.invitee_email.to_owned());

        let claim = TokenClaim::new(
            invitation.id,
            exp,
            TokenType::Invitation,
            TokenRole::OrgMember,
            Some(data),
        );

        Self::try_new(claim)
    }
}

impl JwtToken for InvitationToken {
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
            token_type: claim.token_type,
            role: claim.role,
            email: claim
                .data
                .ok_or_else(|| TokenError::Invitation(anyhow!("Invalid claim")))?
                .get("invitee_email")
                .ok_or_else(|| TokenError::Invitation(anyhow!("Invitee email can't be empty")))?
                .to_string(),
        })
    }

    fn token_type(&self) -> TokenType {
        self.token_type
    }
}

#[tonic::async_trait]
impl Blacklisted for InvitationToken {
    async fn blacklist(&self, conn: &mut diesel_async::AsyncPgConnection) -> TokenResult<bool> {
        let tkn = models::BlacklistToken {
            token: self.encode()?,
            token_type: self.token_type.into(),
        };
        Ok(tkn.create(conn).await.is_ok())
    }

    async fn is_blacklisted(
        &self,
        token: String,
        conn: &mut AsyncPgConnection,
    ) -> TokenResult<bool> {
        Ok(models::BlacklistToken::is_listed(token, conn).await.is_ok())
    }
}

impl FromStr for InvitationToken {
    type Err = TokenError;

    fn from_str(encoded: &str) -> Result<Self, Self::Err> {
        InvitationToken::from_encoded(encoded, TokenType::Invitation, true)
    }
}
