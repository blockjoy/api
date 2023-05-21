use crate::auth::TokenType;
use std::fmt::{Display, Formatter};
use std::fs;
use thiserror::Error;

pub type KeyProviderResult = Result<KeyValue, KeyProviderError>;

#[derive(Error, Debug)]
pub enum KeyProviderError {
    #[error("Key is empty")]
    Empty,
    #[error("Loading environment parameter `{0}` failed with: {1}")]
    EnvError(String, std::env::VarError),
    #[error("Dot env couldn't be loaded: {0}")]
    DotenvError(#[from] dotenv::Error),
    #[error("Key couldn't be loaded from disk: {0}")]
    Disk(#[from] std::io::Error),
    #[error("Unexpected error: {0}")]
    UnexpectedError(#[from] anyhow::Error),
}

#[derive(Debug)]
pub struct KeyValue {
    pub value: String,
}

impl KeyValue {
    pub fn new(value: String) -> Self {
        Self { value }
    }
}

impl Display for KeyValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

pub struct KeyProvider;

impl KeyProvider {
    pub fn get_secret(token_type: TokenType) -> KeyProviderResult {
        let paramname = match token_type {
            TokenType::UserAuth => "JWT_SECRET",
            TokenType::UserRefresh => "REFRESH_SECRET",
            TokenType::HostAuth => "JWT_SECRET",
            TokenType::HostRefresh => "REFRESH_SECRET",
            TokenType::RegistrationConfirmation => "CONFIRMATION_SECRET",
            TokenType::PwdReset => "PWD_RESET_SECRET",
            TokenType::Invitation => "INVITATION_SECRET",
            TokenType::Cookbook => "COOKBOOK_TOKEN",
        };

        let key = Self::get_retriever()(paramname)?;

        if key.value.is_empty() {
            Err(KeyProviderError::Empty)
        } else {
            Ok(key)
        }
    }

    pub fn get_var(name: &str) -> KeyProviderResult {
        let key = Self::get_retriever()(name)?;

        if key.value.is_empty() {
            Err(KeyProviderError::Empty)
        } else {
            Ok(key)
        }
    }

    fn get_retriever() -> fn(&str) -> KeyProviderResult {
        match Self::get_env_value("SECRETS_ROOT") {
            Ok(_) => Self::get_key_value,
            Err(_) => Self::get_env_value,
        }
    }

    fn get_env_value(name: &str) -> KeyProviderResult {
        std::env::var(name)
            .map(KeyValue::new)
            .map_err(|e| KeyProviderError::EnvError(name.to_string(), e))
    }

    fn get_key_value(name: &str) -> KeyProviderResult {
        let path = format!("{}/{}", Self::get_env_value("SECRETS_ROOT")?, name);
        match fs::read_to_string(path).map(KeyValue::new) {
            Ok(value) => Ok(value),
            Err(e) => {
                tracing::error!("Couldn't read key value '{name}' from disk");
                Err(KeyProviderError::Disk(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::key_provider::KeyProvider;
    use crate::auth::TokenType;
    use std::fs;

    #[test]
    fn can_read_secret_from_env() -> anyhow::Result<()> {
        temp_env::with_vars(vec![("JWT_SECRET", Some("123123"))], || {
            let key = KeyProvider::get_secret(TokenType::UserAuth).unwrap();

            assert_eq!("123123", key.to_string());
        });

        Ok(())
    }

    #[test]
    fn can_read_var_from_env() -> anyhow::Result<()> {
        temp_env::with_vars(vec![("DB_URL", Some("lorem"))], || {
            let key = KeyProvider::get_var("DB_URL").expect("Is SECRETS_ROOT set?");

            assert_eq!("lorem", key.to_string());
        });

        Ok(())
    }

    #[test]
    fn can_read_secret_from_file() -> anyhow::Result<()> {
        temp_env::with_vars(
            vec![
                ("JWT_SECRET", Some("098080")),
                ("SECRETS_ROOT", Some("/tmp")),
            ],
            || {
                let path = "/tmp/JWT_SECRET";
                fs::write(path, b"123123").unwrap();

                let key = KeyProvider::get_secret(TokenType::UserAuth).unwrap();

                assert_eq!("123123", key.to_string());

                fs::remove_file(path).unwrap();
            },
        );

        Ok(())
    }

    #[test]
    fn can_read_var_from_file() -> anyhow::Result<()> {
        temp_env::with_vars(
            vec![("DB_URL", Some("lorem")), ("SECRETS_ROOT", Some("/tmp"))],
            || {
                let path = "/tmp/DB_URL";
                fs::write(path, b"ipsum").unwrap();

                let key = KeyProvider::get_var("DB_URL").unwrap();

                assert_eq!("ipsum", key.to_string());

                fs::remove_file(path).unwrap();
            },
        );

        Ok(())
    }
}
