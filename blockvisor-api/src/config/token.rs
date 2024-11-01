use std::convert::Infallible;
use std::str::FromStr;
use std::time::Duration;

use derive_more::Deref;
use displaydoc::Display;
use serde::Deserialize;
use serde_with::{serde_as, DurationSeconds};
use thiserror::Error;

use super::provider::{self, Provider};
use super::{HumanTime, Redacted};

const JWT_SECRET_VAR: &str = "JWT_SECRET";
const JWT_SECRET_ENTRY: &str = "token.secret.jwt";
const REFRESH_SECRET_VAR: &str = "REFRESH_SECRET";
const REFRESH_SECRET_ENTRY: &str = "token.secret.refresh";

const JWT_FALLBACK_SECRET_VAR: &str = "JWT_SECRET_FALLBACK";
const JWT_FALLBACK_SECRET_ENTRY: &str = "token.secret.jwt_fallback";
const REFRESH_FALLBACK_SECRET_VAR: &str = "REFRESH_SECRET_FALLBACK";
const REFRESH_FALLBACK_SECRET_ENTRY: &str = "token.secret.refresh_fallback";

// TODO: delete _MINS consts when the env vars are no longer in use
const TOKEN_EXPIRE_VAR: &str = "TOKEN_EXPIRE";
const TOKEN_EXPIRE_MINS: &str = "TOKEN_EXPIRATION_MINS";
const TOKEN_EXPIRE_ENTRY: &str = "token.expire.token";
const TOKEN_EXPIRE_DEFAULT: &str = "10m";
const REFRESH_HOST_EXPIRE_VAR: &str = "REFRESH_HOST_EXPIRE";
const REFRESH_HOST_EXPIRE_MINS: &str = "REFRESH_EXPIRATION_HOST_MINS";
const REFRESH_HOST_EXPIRE_ENTRY: &str = "token.expire.refresh_host";
const REFRESH_HOST_EXPIRE_DEFAULT: &str = "30d";
const REFRESH_USER_EXPIRE_VAR: &str = "REFRESH_USER_EXPIRE";
const REFRESH_USER_EXPIRE_MINS: &str = "REFRESH_EXPIRATION_USER_MINS";
const REFRESH_USER_EXPIRE_ENTRY: &str = "token.expire.refresh_user";
const REFRESH_USER_EXPIRE_DEFAULT: &str = "20h";
const PASSWORD_RESET_EXPIRE_VAR: &str = "PASSWORD_RESET_EXPIRE";
const PASSWORD_RESET_EXPIRE_MINS: &str = "PWD_RESET_EXPIRATION_MINS";
const PASSWORD_RESET_EXPIRE_ENTRY: &str = "token.expire.password_reset";
const PASSWORD_RESET_EXPIRE_DEFAULT: &str = "5m";
const REGISTRATION_CONFIRMATION_EXPIRE_VAR: &str = "REGISTRATION_CONFIRMATION_EXPIRE";
const REGISTRATION_CONFIRMATION_EXPIRE_MINS: &str = "REGISTRATION_CONFIRMATION_MINS";
const REGISTRATION_CONFIRMATION_EXPIRE_ENTRY: &str = "token.expire.registration_confirmation";
const REGISTRATION_CONFIRMATION_EXPIRE_DEFAULT: &str = "30m";
const INVITATION_EXPIRE_VAR: &str = "INVITATION_EXPIRE";
const INVITATION_EXPIRE_MINS: &str = "INVITATION_MINS";
const INVITATION_EXPIRE_ENTRY: &str = "token.expire.invitation";
const INVITATION_EXPIRE_DEFAULT: &str = "168m";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to convert to chrono::Duration: {0}
    Chrono(chrono::OutOfRangeError),
    /// Failed to parse SecretConfig: {0}
    Secret(#[from] SecretError),
    /// Failed to parse ExpireConfig: {0}
    Expire(#[from] ExpireError),
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub secret: SecretConfig,
    pub expire_config: ExpireConfig,
    pub expire: ExpireChrono,
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let expire_config = provider.try_into()?;
        let expire = ExpireChrono::try_from(expire_config)?;

        Ok(Config {
            secret: provider.try_into()?,
            expire_config,
            expire,
        })
    }
}

#[derive(Debug, Display, Error)]
pub enum SecretError {
    /// Failed to parse {JWT_SECRET_ENTRY:?}: {0}
    ParseJwt(provider::Error),
    /// Failed to parse {REFRESH_SECRET_ENTRY:?}: {0}
    ParseRefresh(provider::Error),
}

#[derive(Debug, Deref, Deserialize, derive_more::FromStr)]
#[deref(forward)]
pub struct JwtSecret(Redacted<String>);

#[derive(Debug, Deref, Deserialize, derive_more::FromStr)]
#[deref(forward)]
pub struct RefreshSecret(Redacted<String>);

#[derive(Debug, Deref, Deserialize, Default)]
#[deref(forward)]
pub struct JwtSecrets(Redacted<Vec<String>>);

impl FromStr for JwtSecrets {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let secrets = s.split(',').map(str::trim).map(str::to_owned).collect();
        Ok(Self(Redacted(secrets)))
    }
}

#[derive(Debug, Deref, Deserialize, Default)]
#[deref(forward)]
pub struct RefreshSecrets(Redacted<Vec<String>>);

impl FromStr for RefreshSecrets {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let secrets = s.split(',').map(str::trim).map(str::to_owned).collect();
        Ok(Self(Redacted(secrets)))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretConfig {
    pub jwt: JwtSecret,
    pub refresh: RefreshSecret,
    pub jwt_fallback: JwtSecrets,
    pub refresh_fallback: RefreshSecrets,
}

impl TryFrom<&Provider> for SecretConfig {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let jwt = provider
            .read(JWT_SECRET_VAR, JWT_SECRET_ENTRY)
            .map_err(SecretError::ParseJwt)?;
        let refresh = provider
            .read(REFRESH_SECRET_VAR, REFRESH_SECRET_ENTRY)
            .map_err(SecretError::ParseRefresh)?;
        let jwt_fallback = provider
            .read_or_default(JWT_FALLBACK_SECRET_VAR, JWT_FALLBACK_SECRET_ENTRY)
            .map_err(SecretError::ParseJwt)?;
        let refresh_fallback = provider
            .read_or_default(REFRESH_FALLBACK_SECRET_VAR, REFRESH_FALLBACK_SECRET_ENTRY)
            .map_err(SecretError::ParseRefresh)?;

        Ok(SecretConfig {
            jwt,
            refresh,
            jwt_fallback,
            refresh_fallback,
        })
    }
}

#[derive(Debug, Display, Error)]
pub enum ExpireError {
    /// Failed to parse {TOKEN_EXPIRE_ENTRY:?}: {0}
    Token(provider::Error),
    /// Failed to parse {REFRESH_HOST_EXPIRE_ENTRY:?}: {0}
    RefreshHost(provider::Error),
    /// Failed to parse {REFRESH_USER_EXPIRE_ENTRY:?}: {0}
    RefreshUser(provider::Error),
    /// Failed to parse {PASSWORD_RESET_EXPIRE_ENTRY:?}: {0}
    PasswordReset(provider::Error),
    /// Failed to parse {REGISTRATION_CONFIRMATION_EXPIRE_ENTRY:?}: {0}
    RegistrationConfirmation(provider::Error),
    /// Failed to parse {INVITATION_EXPIRE_ENTRY:?}: {0}
    Invitation(provider::Error),
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpireConfig {
    pub token: HumanTime,
    pub refresh_host: HumanTime,
    pub refresh_user: HumanTime,
    pub password_reset: HumanTime,
    pub registration_confirmation: HumanTime,
    pub invitation: HumanTime,
}

impl TryFrom<&Provider> for ExpireConfig {
    type Error = ExpireError;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let read_mins = |var| provider.read_var::<u64, _>(var);

        let token = provider
            .read_or_else(
                || {
                    if let Ok(mins) = read_mins(TOKEN_EXPIRE_MINS) {
                        Ok(Duration::from_secs(60 * mins).into())
                    } else {
                        TOKEN_EXPIRE_DEFAULT.parse::<HumanTime>()
                    }
                },
                TOKEN_EXPIRE_VAR,
                TOKEN_EXPIRE_ENTRY,
            )
            .map_err(ExpireError::Token)?;
        let refresh_host = provider
            .read_or_else(
                || {
                    if let Ok(mins) = read_mins(REFRESH_HOST_EXPIRE_MINS) {
                        Ok(Duration::from_secs(60 * mins).into())
                    } else {
                        REFRESH_HOST_EXPIRE_DEFAULT.parse::<HumanTime>()
                    }
                },
                REFRESH_HOST_EXPIRE_VAR,
                REFRESH_HOST_EXPIRE_ENTRY,
            )
            .map_err(ExpireError::RefreshHost)?;
        let refresh_user = provider
            .read_or_else(
                || {
                    if let Ok(mins) = read_mins(REFRESH_USER_EXPIRE_MINS) {
                        Ok(Duration::from_secs(60 * mins).into())
                    } else {
                        REFRESH_USER_EXPIRE_DEFAULT.parse::<HumanTime>()
                    }
                },
                REFRESH_USER_EXPIRE_VAR,
                REFRESH_USER_EXPIRE_ENTRY,
            )
            .map_err(ExpireError::RefreshUser)?;
        let password_reset = provider
            .read_or_else(
                || {
                    if let Ok(mins) = read_mins(PASSWORD_RESET_EXPIRE_MINS) {
                        Ok(Duration::from_secs(60 * mins).into())
                    } else {
                        PASSWORD_RESET_EXPIRE_DEFAULT.parse::<HumanTime>()
                    }
                },
                PASSWORD_RESET_EXPIRE_VAR,
                PASSWORD_RESET_EXPIRE_ENTRY,
            )
            .map_err(ExpireError::PasswordReset)?;
        let registration_confirmation = provider
            .read_or_else(
                || {
                    if let Ok(mins) = read_mins(REGISTRATION_CONFIRMATION_EXPIRE_MINS) {
                        Ok(Duration::from_secs(60 * mins).into())
                    } else {
                        REGISTRATION_CONFIRMATION_EXPIRE_DEFAULT.parse::<HumanTime>()
                    }
                },
                REGISTRATION_CONFIRMATION_EXPIRE_VAR,
                REGISTRATION_CONFIRMATION_EXPIRE_ENTRY,
            )
            .map_err(ExpireError::RegistrationConfirmation)?;
        let invitation = provider
            .read_or_else(
                || {
                    if let Ok(mins) = read_mins(INVITATION_EXPIRE_MINS) {
                        Ok(Duration::from_secs(60 * mins).into())
                    } else {
                        INVITATION_EXPIRE_DEFAULT.parse::<HumanTime>()
                    }
                },
                INVITATION_EXPIRE_VAR,
                INVITATION_EXPIRE_ENTRY,
            )
            .map_err(ExpireError::Invitation)?;

        Ok(ExpireConfig {
            token,
            refresh_host,
            refresh_user,
            password_reset,
            registration_confirmation,
            invitation,
        })
    }
}

#[serde_as]
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ExpireChrono {
    #[serde_as(as = "DurationSeconds<i64>")]
    pub token: chrono::Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub refresh_host: chrono::Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub refresh_user: chrono::Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub password_reset: chrono::Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub registration_confirmation: chrono::Duration,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub invitation: chrono::Duration,
}

impl TryFrom<ExpireConfig> for ExpireChrono {
    type Error = Error;

    fn try_from(config: ExpireConfig) -> Result<Self, Self::Error> {
        Ok(ExpireChrono {
            token: chrono::Duration::from_std(*config.token).map_err(Error::Chrono)?,
            refresh_host: chrono::Duration::from_std(*config.refresh_host)
                .map_err(Error::Chrono)?,
            refresh_user: chrono::Duration::from_std(*config.refresh_user)
                .map_err(Error::Chrono)?,
            password_reset: chrono::Duration::from_std(*config.password_reset)
                .map_err(Error::Chrono)?,
            registration_confirmation: chrono::Duration::from_std(
                *config.registration_confirmation,
            )
            .map_err(Error::Chrono)?,
            invitation: chrono::Duration::from_std(*config.invitation).map_err(Error::Chrono)?,
        })
    }
}
