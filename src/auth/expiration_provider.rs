use crate::auth::TokenType;
use crate::errors::{ApiError, Result as ApiResult};
use anyhow::anyhow;
use chrono::{Duration, Utc};

pub struct ExpirationProvider;

impl ExpirationProvider {
    pub fn expiration(token_type: TokenType) -> i64 {
        let value = match token_type {
            TokenType::UserAuth => Self::get_expiration_from_dotenv("TOKEN_EXPIRATION_MINS_USER"),
            TokenType::UserRefresh => {
                Self::get_expiration_from_dotenv("REFRESH_TOKEN_EXPIRATION_MINS_USER")
            }
            TokenType::PwdReset => {
                Self::get_expiration_from_dotenv("PWD_RESET_TOKEN_EXPIRATION_MINS_USER")
            }
            TokenType::RegistrationConfirmation => {
                Self::get_expiration_from_dotenv("REGISTRATION_CONFIRMATION_MINS_USER")
            }
            TokenType::HostAuth => Self::get_expiration_from_dotenv("TOKEN_EXPIRATION_MINS_HOST"),
            TokenType::HostRefresh => {
                Self::get_expiration_from_dotenv("REFRESH_EXPIRATION_MINS_HOST")
            }
        };

        value.unwrap_or(0)
    }

    fn get_expiration_from_dotenv(key: &str) -> ApiResult<i64> {
        let now = Utc::now();
        let duration = Duration::minutes(
            dotenv::var(key)
                .map_err(ApiError::EnvError)?
                .parse::<i64>()
                .map_err(|e| {
                    ApiError::UnexpectedError(anyhow!("Couldn't parse env var value: {e:?}"))
                })?,
        );
        let expiration = (now + duration).timestamp();

        Ok(expiration)
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::TokenType;
    use strum::IntoEnumIterator;

    #[test]
    fn can_return_valid_expiration_for_each_token_type() {
        for tt in TokenType::iter() {
            println!("Testing token type: {}", tt);
            assert!(super::ExpirationProvider::expiration(tt) > 0)
        }
    }
}
