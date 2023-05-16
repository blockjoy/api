#![allow(unused)] // this code is a stub

//! This module contains our implementation of api keys. Api keys are long-lived authentication,
//! whose permissions are consequently retrieved from the database. Their format is of the following
//! form: `blockjoy_EhLVJvop1fYnPDbWN/b/vg_mIsj3FhLEt4NtlY5`. The constitutes three parts, seperated
//! using an `_`.
//!
//! - `blockjoy`: This is the word blockjoy, and it is included it because it is useful for noticing
//!   to which system the api key belongs.
//! - `EhLVJvop1fYnPDbWN/b/vg`: This is a base64 encoded u128, and therefore a more compact notation
//!   for a UUID. This UUID is the primary key of the corresponding API key record in the database.
//! - `mIsj3FhLEt4NtlY5`: This is a randomly generated, alphanumeric string of 20 characters. This
//!   acts as a sort of `password`, and it is the secret part of the api key. It is hashed an salted
//!   before being stored in the database. It is equal to (26 + 26 + 26)^20 = 7 * 10^36 bits entropy
//!   equivalent, which is deemed to be enough.

pub struct ApiKey {
    key: String,
}

impl ApiKey {
    pub fn encode(self) -> crate::Result<String> {
        Ok(self.key)
    }

    pub fn decode(raw: &str) -> crate::Result<Self> {
        Ok(Self {
            key: "Chris will implement this".to_string(),
        })
    }
}
