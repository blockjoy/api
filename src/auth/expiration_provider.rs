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
    use crate::auth;
    use chrono::{Duration, Utc};

    #[test]
    fn can_calculate_expiration_time() {
        temp_env::with_vars(vec![(auth::TOKEN_EXPIRATION_MINS, Some("10"))], || {
            let now = Utc::now();
            let duration = Duration::minutes(
                dotenv::var(auth::TOKEN_EXPIRATION_MINS)
                    .unwrap()
                    .parse::<i64>()
                    .unwrap(),
            );
            let expiration = (now + duration).timestamp();

            println!("Now: {}, expires: {}", now.timestamp(), expiration);
            assert_eq!(duration.num_minutes(), 10);
            assert!(expiration > now.timestamp());
        });
    }
}
