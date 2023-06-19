use std::time::Duration;

use derive_more::{Deref, FromStr};
use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;

use super::provider::{self, Provider};
use super::{HumanTime, Redacted};

const JWT_SECRET_VAR: &str = "JWT_SECRET";
const JWT_SECRET_ENTRY: &str = "token.secret.jwt";
const PASSWORD_RESET_SECRET_VAR: &str = "PWD_RESET_SECRET";
const PASSWORD_RESET_SECRET_ENTRY: &str = "token.secret.password_reset";

// TODO: delete _MINS consts when the env vars are no longer in use
const TOKEN_EXPIRE_VAR: &str = "TOKEN_EXPIRE";
const TOKEN_EXPIRE_MINS: &str = "TOKEN_EXPIRATION_MINS";
const TOKEN_EXPIRE_ENTRY: &str = "token.expire.token";
const TOKEN_EXPIRE_DEFAULT: &str = "10m";
const EXPIRE_REFRESH_VAR: &str = "EXPIRE_REFRESH";
const EXPIRE_REFRESH_MINS: &str = "REFRESH_EXPIRATION_MINS";
const EXPIRE_REFRESH_ENTRY: &str = "token.expire.refresh";
const EXPIRE_REFRESH_DEFAULT: &str = "20h";
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
    /// Failed to parse SecretConfig: {0}
    Secret(#[from] SecretError),
    /// Failed to parse ExpireConfig: {0}
    Expire(#[from] ExpireError),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub secret: SecretConfig,
    pub expire: ExpireConfig,
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(Config {
            secret: provider.try_into()?,
            expire: provider.try_into()?,
        })
    }
}

#[derive(Debug, Display, Error)]
pub enum SecretError {
    /// Failed to parse ${JWT_SECRET_ENTRY:?}: {0}
    ParseJwt(provider::Error),
    /// Failed to parse ${PASSWORD_RESET_SECRET_ENTRY:?}: {0}
    ParsePasswordReset(provider::Error),
}

#[derive(Debug, Deref, Deserialize, FromStr)]
#[deref(forward)]
pub struct JwtSecret(Redacted<String>);

#[derive(Debug, Deref, Deserialize, FromStr)]
#[deref(forward)]
pub struct PasswordResetSecret(Redacted<String>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretConfig {
    pub jwt: JwtSecret,
    pub password_reset: PasswordResetSecret,
}

impl TryFrom<&Provider> for SecretConfig {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        let jwt = provider
            .read(JWT_SECRET_VAR, JWT_SECRET_ENTRY)
            .map_err(SecretError::ParseJwt)?;
        let password_reset = provider
            .read(PASSWORD_RESET_SECRET_VAR, PASSWORD_RESET_SECRET_ENTRY)
            .map_err(SecretError::ParsePasswordReset)?;

        Ok(SecretConfig {
            jwt,
            password_reset,
        })
    }
}

#[derive(Debug, Display, Error)]
pub enum ExpireError {
    /// Failed to parse ${TOKEN_EXPIRE_ENTRY:?}: {0}
    Token(provider::Error),
    /// Failed to parse ${EXPIRE_REFRESH_ENTRY:?}: {0}
    Refresh(provider::Error),
    /// Failed to parse ${REFRESH_HOST_EXPIRE_ENTRY:?}: {0}
    RefreshHost(provider::Error),
    /// Failed to parse ${REFRESH_USER_EXPIRE_ENTRY:?}: {0}
    RefreshUser(provider::Error),
    /// Failed to parse ${PASSWORD_RESET_EXPIRE_ENTRY:?}: {0}
    PasswordReset(provider::Error),
    /// Failed to parse ${REGISTRATION_CONFIRMATION_EXPIRE_ENTRY:?}: {0}
    RegistrationConfirmation(provider::Error),
    /// Failed to parse ${INVITATION_EXPIRE_ENTRY:?}: {0}
    Invitation(provider::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpireConfig {
    pub token: HumanTime,
    pub refresh: HumanTime,
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
        let refresh = provider
            .read_or_else(
                || {
                    if let Ok(mins) = read_mins(EXPIRE_REFRESH_MINS) {
                        Ok(Duration::from_secs(60 * mins).into())
                    } else {
                        EXPIRE_REFRESH_DEFAULT.parse::<HumanTime>()
                    }
                },
                EXPIRE_REFRESH_VAR,
                EXPIRE_REFRESH_ENTRY,
            )
            .map_err(ExpireError::Refresh)?;
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
            refresh,
            refresh_host,
            refresh_user,
            password_reset,
            registration_confirmation,
            invitation,
        })
    }
}
