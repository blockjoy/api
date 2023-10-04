use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{env, fs};

use displaydoc::Display;
use thiserror::Error;
use toml::{Table, Value};
use tracing::log::debug;

const SECRETS_ROOT: &str = "SECRETS_ROOT";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Config file is empty: {0}
    EmptyFile(PathBuf),
    /// Environment variable is empty: {0}
    EmptyVar(String),
    /// Failed to load environment variable `{0}`: {1}
    EnvVar(String, env::VarError),
    /// Failed to read metadata for file: {0}
    FileMeta(PathBuf, std::io::Error),
    /// No config entry: {0}
    NoEntry(&'static str),
    /// No config file at path: {0}
    NoFile(PathBuf),
    /// No secrets_root file.
    NoSecretsRoot,
    /// Config source directory does not exist: {0}
    NoSourceDir(PathBuf),
    /// No toml entry: {0}
    NoTomlEntry(&'static str),
    /// No toml config file.
    NoTomlFile,
    /// No environment variable set: {0}
    NoVar(String),
    /// Failed to parse default value: {0}
    ParseDefault(Box<dyn std::error::Error + Send + Sync + 'static>),
    /// Failed to parse environment file `{0}`: {1}
    ParseFile(PathBuf, Box<dyn std::error::Error + Send + Sync + 'static>),
    /// Failed to parse toml::Value::String at `{0}`: {1}
    ParseTomlString(
        &'static str,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    ),
    /// Failed to parse toml::Value at `{0}`: {1}
    ParseTomlValue(
        &'static str,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    ),
    /// Failed to parse environment variable `{0}`: {1}
    ParseVar(String, Box<dyn std::error::Error + Send + Sync + 'static>),
    /// Failed to read file `{0}`: {1}
    ReadFile(PathBuf, std::io::Error),
    /// Failed to read toml `{0}`: {1}
    ReadToml(PathBuf, std::io::Error),
    /// Failed to read toml table: {0}
    TomlTable(toml::de::Error),
}

/// Provider will retrieve config values from the environment.
///
/// The order of precedence is to first check under `SECRETS_ROOT`, then check
/// for an environment variable, and finally to check the toml file.
pub struct Provider {
    secrets_root: Option<PathBuf>,
    toml_table: Option<Table>,
}

impl Provider {
    pub fn new<P: AsRef<Path>>(toml: Option<P>) -> Result<Self, Error> {
        let secrets_root = if let Ok(path) = env::var(SECRETS_ROOT) {
            debug!("Parsing config from directory: `{SECRETS_ROOT}`");
            Some(Self::secrets_root(path)?)
        } else {
            debug!("No `{SECRETS_ROOT}` directory exists.");
            None
        };

        let toml_table = if let Some(file) = toml {
            debug!("Parsing additional config from file: `{:?}`", file.as_ref());
            Some(Self::toml_table(file)?)
        } else {
            debug!("No `config.toml` file exists.");
            None
        };

        Ok(Provider {
            secrets_root,
            toml_table,
        })
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn from_toml<P: AsRef<Path>>(toml: P) -> Result<Self, Error> {
        Ok(Provider {
            secrets_root: None,
            toml_table: Some(Self::toml_table(toml)?),
        })
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn from_tmp() -> Result<Self, Error> {
        Ok(Provider {
            secrets_root: Some(Self::secrets_root("/tmp")?),
            toml_table: None,
        })
    }

    fn secrets_root<P: AsRef<Path>>(path: P) -> Result<PathBuf, Error> {
        let path = path.as_ref().to_path_buf();
        if path.is_dir() {
            Ok(path)
        } else {
            Err(Error::NoSourceDir(path))
        }
    }

    fn toml_table<P: AsRef<Path>>(toml: P) -> Result<Table, Error> {
        let toml = toml.as_ref();
        let file = fs::read_to_string(toml).map_err(|err| Error::ReadToml(toml.into(), err))?;
        toml::from_str(&file).map_err(Error::TomlTable)
    }

    /// Parse the config variable defined by `var` or `entry`.
    pub fn read<T, E>(&self, var: &str, entry: &'static str) -> Result<T, Error>
    where
        T: FromStr<Err = E>,
        E: std::error::Error + Send + Sync + 'static,
    {
        self.read_fn(var, entry, None::<fn() -> Result<T, E>>)
    }

    /// Parse the config variable defined by `var` or `entry`.
    ///
    /// If the config variable does not exist, return the default value instead.
    pub fn read_or<S, T, E>(&self, default: S, var: &str, entry: &'static str) -> Result<T, Error>
    where
        S: Into<T>,
        T: FromStr<Err = E>,
        E: std::error::Error + Send + Sync + 'static,
    {
        self.read_fn(var, entry, Some(|| Ok::<S, E>(default)))
    }

    /// Parse the config variable defined by `var` or `entry`.
    ///
    /// If the config variable does not exist, return `T::default` instead.
    pub fn read_or_default<T, E>(&self, var: &str, entry: &'static str) -> Result<T, Error>
    where
        T: Default + FromStr<Err = E>,
        E: std::error::Error + Send + Sync + 'static,
    {
        self.read_fn(var, entry, Some(|| Ok::<T, E>(T::default())))
    }

    /// Parse the config variable defined by `var` or `entry`.
    ///
    /// If the config variable does not exist, return the default result instead.
    pub fn read_or_else<F, S, T, E>(
        &self,
        default: F,
        var: &str,
        entry: &'static str,
    ) -> Result<T, Error>
    where
        F: FnOnce() -> Result<S, E>,
        S: Into<T>,
        T: FromStr<Err = E>,
        E: std::error::Error + Send + Sync + 'static,
    {
        self.read_fn(var, entry, Some(default))
    }

    /// Parse the config variable defined by `var` or `entry`.
    ///
    /// If the config variable does not exist, return the default result if defined.
    fn read_fn<F, S, T, E>(
        &self,
        var: &str,
        entry: &'static str,
        default: Option<F>,
    ) -> Result<T, Error>
    where
        F: FnOnce() -> Result<S, E>,
        S: Into<T>,
        T: FromStr<Err = E>,
        E: std::error::Error + Send + Sync + 'static,
    {
        match self.read_file(var) {
            Ok(parsed) => return Ok(parsed),
            Err(Error::NoFile(..) | Error::NoSecretsRoot) => (),
            Err(err) => return Err(err),
        }

        match self.read_var(var) {
            Ok(parsed) => return Ok(parsed),
            Err(Error::NoVar(..)) => (),
            Err(err) => return Err(err),
        }

        match self.read_toml(entry) {
            Ok(parsed) => return Ok(parsed),
            Err(Error::NoTomlEntry(..) | Error::NoTomlFile) => (),
            Err(err) => return Err(err),
        }

        match default {
            Some(default) => default()
                .map(Into::into)
                .map_err(|err| Error::ParseDefault(Box::new(err))),
            None => Err(Error::NoEntry(entry)),
        }
    }

    /// Parse the variable from the contents of the file at `var`.
    fn read_file<T, E>(&self, var: &str) -> Result<T, Error>
    where
        T: FromStr<Err = E>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let file = if let Some(root) = &self.secrets_root {
            root.join(var)
        } else {
            return Err(Error::NoSecretsRoot);
        };

        match fs::metadata(&file) {
            Ok(meta) if meta.len() == 0 => Err(Error::EmptyFile(file.clone())),
            Ok(_) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => Err(Error::NoFile(file.clone())),
            Err(err) => Err(Error::FileMeta(file.clone(), err)),
        }?;

        fs::read_to_string(&file)
            .map_err(|err| Error::ReadFile(file.clone(), err))
            .and_then(|val| {
                val.parse()
                    .map_err(|err| Error::ParseFile(file, Box::new(err)))
            })
    }

    /// Parse the contents of the environment variable `var`.
    #[allow(clippy::unused_self)]
    pub(super) fn read_var<T, E>(&self, var: &str) -> Result<T, Error>
    where
        T: FromStr<Err = E>,
        E: std::error::Error + Send + Sync + 'static,
    {
        match env::var(var) {
            Ok(val) if val.is_empty() => Err(Error::EmptyVar(var.into())),
            Ok(val) => val
                .parse()
                .map_err(|err| Error::ParseVar(var.into(), Box::new(err))),
            Err(_) => Err(Error::NoVar(var.into())),
        }
    }

    /// Parse the contents of the toml variable defined at `entry`.
    fn read_toml<T, E>(&self, entry: &'static str) -> Result<T, Error>
    where
        T: FromStr<Err = E>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let mut table = self.toml_table.as_ref().ok_or(Error::NoTomlFile)?;

        for path in entry.split('.') {
            match table.get(path) {
                Some(Value::Table(t)) => table = t,
                Some(Value::String(s)) => {
                    return s
                        .parse()
                        .map_err(|err| Error::ParseTomlString(entry, Box::new(err)))
                }
                Some(val) => {
                    return val
                        .to_string()
                        .parse()
                        .map_err(|err| Error::ParseTomlValue(entry, Box::new(err)))
                }
                None => return Err(Error::NoTomlEntry(entry)),
            }
        }

        Err(Error::NoTomlEntry(entry))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::config::Config;

    #[test]
    fn can_read_var_from_file() {
        let env = [(SECRETS_ROOT, Some("/tmp")), ("JWT_SECRET", Some("098080"))];

        let path = "/tmp/JWT_SECRET";
        fs::write(path, b"123123").unwrap();

        temp_env::with_vars(env, || {
            let config = Config::new().unwrap();
            assert_eq!(*config.token.secret.jwt, "123123");
        });

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn can_read_var_from_env() {
        let env = [("JWT_SECRET", Some("123123"))];

        temp_env::with_vars(env, || {
            let config = Config::new().unwrap();
            assert_eq!(*config.token.secret.jwt, "123123");
        });
    }

    #[test]
    fn can_read_var_from_toml() {
        let config = Config::from_default_toml().unwrap();
        assert_eq!(config.grpc.request_concurrency_limit, 128);
    }
}
