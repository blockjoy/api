//! API keys are an alternative form of authentication instead of a login
//! cookie.
//!
//! Their permissions are constrained to a subset of the creator's permissions
//! determined by the `resource` and `resource_id` fields in the database.
//!
//! The API key format is of the form: `blockjoy_{key_id}_{secret}`, where:
//!
//! `{key_id}` is a base64-encoded representation of the UUID used for looking
//! up the `KeyId` row from the database.
//! `{secret}` is a base64-encoded representation of the secret bytes, which
//! when hashed together with the database `key_salt` field should equal the
//! database `key_hash` field.

use std::str::FromStr;

use base64::engine::{general_purpose::STANDARD_NO_PAD, Engine as _};
use derive_more::{Deref, Into};
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display;
use rand::RngCore;
use thiserror::Error;
use uuid::Uuid;
use zeroize::ZeroizeOnDrop;

use crate::auth::claims::Claims;
use crate::auth::rbac::Perms;
use crate::auth::resource::Resource;
use crate::auth::token::ApiToken;
use crate::database::Conn;
use crate::model::ApiKey;

pub(super) const TOKEN_PREFIX: &str = "api_";
const TOKEN_ID_LEN: usize = 22;
const TOKEN_SECRET_LEN: usize = 27;
const TOKEN_LEN: usize = TOKEN_PREFIX.len() + TOKEN_ID_LEN + 1 + TOKEN_SECRET_LEN;
const SALT_BYTES: usize = 16;
const SECRET_BYTES: usize = 20;

/// Internal errors. Note that these are not safe for external display.
#[derive(Debug, Display, Error)]
pub enum Error {
    /// Invalid decoded secret length.
    BadSecretLen,
    /// Invalid api key token length.
    BadTokenLen,
    /// Failed to parse key id as base64.
    DecodeKeyId(base64::DecodeError),
    /// Failed to parse secret as base64.
    DecodeSecret(base64::DecodeError),
    /// Failed to find KeyId: {0}
    FindKeyId(crate::model::api_key::Error),
    /// Key hash mismatch.
    HashMismatch,
    /// Failed to parse KeyId: {0}
    ParseKeyId(uuid::Error),
}

/// A validated ownership of some `ApiKey`.
#[derive(Debug, Deref)]
pub struct Validated(ApiKey);

impl Validated {
    pub async fn from_token(token: &ApiToken, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let api_key = ApiKey::by_id(token.key_id, conn)
            .await
            .map_err(Error::FindKeyId)?;

        let key_hash = KeyHash::from(&api_key.key_salt, &token.secret);
        if key_hash != api_key.key_hash {
            return Err(Error::HashMismatch);
        }

        Ok(Validated(api_key))
    }

    pub fn claims(self, expires: chrono::Duration) -> Claims {
        let resource = Resource::from(&self.0);
        let perms = Perms::from(self.0.permissions);
        Claims::from_now(expires, resource, perms)
    }
}

/// A newtype represenation of the database `id`.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, DieselNewType, Deref)]
pub struct KeyId(Uuid);

impl KeyId {
    pub fn from_token(token: &str) -> Result<Self, Error> {
        if token.len() != TOKEN_LEN {
            return Err(Error::BadTokenLen);
        }

        let start = TOKEN_PREFIX.len();
        let end = start + TOKEN_ID_LEN;

        let id_bytes: Vec<u8> = STANDARD_NO_PAD
            .decode(&token[start..end])
            .map_err(Error::DecodeKeyId)?;

        Uuid::from_slice(&id_bytes)
            .map(KeyId)
            .map_err(Error::ParseKeyId)
    }
}

impl FromStr for KeyId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(KeyId).map_err(Error::ParseKeyId)
    }
}

/// A base64-encoded representation of the hash of the salt and secret.
#[derive(Debug, PartialEq, Eq, DieselNewType)]
pub struct KeyHash(String);

impl KeyHash {
    pub fn from(salt: &Salt, secret: &Secret) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(salt.0.as_bytes());
        hasher.update(&secret.0);

        let hash = hasher.finalize();
        let encoded = STANDARD_NO_PAD.encode(hash.as_bytes());

        KeyHash(encoded)
    }
}

/// A newtype wrapping the database `salt` text.
#[derive(Debug, DieselNewType)]
pub struct Salt(String);

impl Salt {
    pub fn generate<R: RngCore>(rng: &mut R) -> Self {
        let mut salt = [0u8; SALT_BYTES];
        rng.fill_bytes(&mut salt);
        Salt(STANDARD_NO_PAD.encode(salt))
    }
}

/// This contains the API Key secret bytes.
///
/// This must not be stored and the memory bytes are zeroed on Drop.
#[derive(ZeroizeOnDrop)]
pub struct Secret([u8; SECRET_BYTES]);

impl Secret {
    pub fn generate<R: RngCore>(rng: &mut R) -> Self {
        let mut secret = [0u8; SECRET_BYTES];
        rng.fill_bytes(&mut secret);
        Secret(secret)
    }

    pub fn from_token(token: &str) -> Result<Self, Error> {
        if token.len() != TOKEN_LEN {
            return Err(Error::BadTokenLen);
        }

        let start = TOKEN_LEN - TOKEN_SECRET_LEN;
        let secret: [u8; SECRET_BYTES] = STANDARD_NO_PAD
            .decode(&token[start..])
            .map_err(Error::DecodeSecret)?
            .try_into()
            .map_err(|_| Error::BadSecretLen)?;

        Ok(Secret(secret))
    }
}

/// An encoded string representation of the API key.
///
/// This must not be stored and is only returned once on creation.
#[derive(Into)]
pub struct BearerSecret(String);

impl BearerSecret {
    pub fn new(key_id: KeyId, secret: &Secret) -> Self {
        let key_id: String = STANDARD_NO_PAD.encode(key_id.0);
        let secret: String = STANDARD_NO_PAD.encode(secret.0);

        BearerSecret(format!("{TOKEN_PREFIX}{key_id}_{secret}"))
    }
}
