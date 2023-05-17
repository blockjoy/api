use std::fs;
use thiserror::Error;

pub type KeyProviderResult = Result<String, KeyProviderError>;

const SECRETS_ROOT: &str = "SECRETS_ROOT";
const JWT_SECRET: &str = "JWT_SECRET";
const REFRESH_SECRET: &str = "REFRESH_SECRET";

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

pub struct KeyProvider;

impl KeyProvider {
    pub fn jwt_secret() -> KeyProviderResult {
        Self::get_var(JWT_SECRET)
    }

    pub fn refresh_secret() -> KeyProviderResult {
        Self::get_var(REFRESH_SECRET)
    }

    pub fn get_var(name: &str) -> KeyProviderResult {
        let key = Self::get_retriever()(name)?;

        if key.is_empty() {
            Err(KeyProviderError::Empty)
        } else {
            Ok(key)
        }
    }

    fn get_retriever() -> fn(&str) -> KeyProviderResult {
        match Self::get_env_value(SECRETS_ROOT) {
            Ok(_) => Self::get_key_value,
            Err(_) => Self::get_env_value,
        }
    }

    fn get_env_value(name: &str) -> KeyProviderResult {
        std::env::var(name).map_err(|e| KeyProviderError::EnvError(name.to_string(), e))
    }

    fn get_key_value(name: &str) -> KeyProviderResult {
        let path = format!("{}/{}", Self::get_env_value(SECRETS_ROOT)?, name);
        match fs::read_to_string(path) {
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
    use super::*;
    use std::fs;

    #[test]
    fn can_read_secret_from_env() {
        temp_env::with_vars([(JWT_SECRET, Some("123123"))], || {
            let key = KeyProvider::jwt_secret().unwrap();
            assert_eq!("123123", key);
        })
    }

    #[test]
    fn can_read_var_from_env() {
        temp_env::with_vars([("DB_URL", Some("lorem"))], || {
            let key = KeyProvider::get_var("DB_URL").expect("Is SECRETS_ROOT set?");
            assert_eq!("lorem", key);
        })
    }

    #[test]
    fn can_read_secret_from_file() {
        temp_env::with_vars(
            vec![(JWT_SECRET, Some("098080")), (SECRETS_ROOT, Some("/tmp"))],
            || {
                let path = "/tmp/JWT_SECRET";
                fs::write(path, b"123123").unwrap();
                let key = KeyProvider::jwt_secret().unwrap();
                assert_eq!("123123", key);
                fs::remove_file(path).unwrap();
            },
        )
    }

    #[test]
    fn can_read_var_from_file() {
        temp_env::with_vars(
            vec![("DB_URL", Some("lorem")), (SECRETS_ROOT, Some("/tmp"))],
            || {
                let path = "/tmp/DB_URL";
                fs::write(path, b"ipsum").unwrap();
                let key = KeyProvider::get_var("DB_URL").unwrap();
                assert_eq!("ipsum", key);
                fs::remove_file(path).unwrap();
            },
        )
    }
}
