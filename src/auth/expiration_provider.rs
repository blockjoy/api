//! This module contains the `ExpirationProvider` struct, which encapsulates our way of retrieving
//! the expirations of various tokens. The current way we do this is by querying them from the
//! environment.

pub struct ExpirationProvider;

impl ExpirationProvider {
    pub fn expiration(name: &str) -> crate::Result<chrono::Duration> {
        let val = super::key_provider::KeyProvider::get_var(name)?;
        let val = val.parse()?;
        let val = chrono::Duration::minutes(val);
        Ok(val)
    }
}

#[cfg(test)]
mod tests {
    use crate::Error;
    use anyhow::anyhow;
    use chrono::{Duration, Utc};

    #[test]
    fn can_calculate_expiration_time() -> anyhow::Result<()> {
        temp_env::with_vars(vec![("TOKEN_EXPIRATION_MINS_USER", Some("10"))], || {
            let now = Utc::now();
            let duration = Duration::minutes(
                dotenv::var("TOKEN_EXPIRATION_MINS_USER")
                    .map_err(Error::EnvError)?
                    .parse::<i64>()
                    .map_err(|e| {
                        Error::UnexpectedError(anyhow!("Couldn't parse env var value: {e:?}"))
                    })?,
            );
            let expiration = (now + duration).timestamp();

            println!("Now: {}, expires: {}", now.timestamp(), expiration);
            assert_eq!(duration.num_minutes(), 10);
            assert!(expiration > now.timestamp());

            Ok(())
        })
    }
}
