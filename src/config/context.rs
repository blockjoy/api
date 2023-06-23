use std::sync::Arc;

use displaydoc::Display;
use thiserror::Error;

use super::Config;
use crate::auth::token::Cipher;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to build Config: {0}
    Config(super::Error),
}

/// Service `Context` containing metadata that can be passed down to handlers.
///
/// Each field is wrapped in an Arc so modules may clone them to retain in their
/// own struct state as necessary.
#[derive(Clone)]
pub struct Context {
    pub config: Arc<Config>,
    pub cipher: Arc<Cipher>,
}

impl Context {
    pub fn new() -> Result<Arc<Self>, Error> {
        let config = Config::new().map_err(Error::Config)?;
        let cipher = Arc::new(Cipher::new(&config.token.secret));

        Ok(Arc::new(Context { config, cipher }))
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn new_with_toml<P: AsRef<std::path::Path>>(toml: P) -> Result<Arc<Self>, Error> {
        let config = Config::new_with_toml(toml).map_err(Error::Config)?;
        let cipher = Arc::new(Cipher::new(&config.token.secret));

        Ok(Arc::new(Context { config, cipher }))
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn new_with_default_toml() -> Result<Arc<Self>, Error> {
        Self::new_with_toml(super::CONFIG_FILE)
    }
}
