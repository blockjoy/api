use crate::grpc::api;
use crate::models::node::{NodeType, NodeVersion};

use displaydoc::Display;
use semver::Version;
use thiserror::Error;
use tracing::trace;

use super::{BUNDLE_NAME, KERNEL_NAME};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse NodeType: {0}
    ParseNodeType(crate::models::node::node_type::Error),
    /// Failed to parse semantic version from key `{0}`: {1}
    ParseVersion(String, semver::Error),
    /// Key `{0}` is not splittable into at least 4 `/` separated parts.
    SplitKey(String),
    /// File name should end in `/{BUNDLE_NAME:?}` but is `{0}`.
    SuffixBundle(String),
    /// File name should end in `/{KERNEL_NAME:?}` but is `{0}`.
    SuffixKernel(String),
}

/// A Cookbook plugin identifier.
#[derive(Clone, Debug)]
pub struct Identifier {
    pub protocol: String,
    pub node_type: NodeType,
    pub node_version: NodeVersion,
}

impl Identifier {
    pub fn new<S: Into<String>>(
        protocol: S,
        node_type: NodeType,
        node_version: NodeVersion,
    ) -> Self {
        Identifier {
            protocol: protocol.into(),
            node_type,
            node_version,
        }
    }

    pub fn node_version(&self) -> NodeVersion {
        self.node_version.to_string().into()
    }
}

impl From<api::ConfigIdentifier> for Identifier {
    fn from(api: api::ConfigIdentifier) -> Self {
        let node_type = api.node_type().into_model();

        Identifier {
            protocol: api.protocol,
            node_type,
            node_version: api.node_version.into(),
        }
    }
}

impl From<Identifier> for api::ConfigIdentifier {
    fn from(id: Identifier) -> Self {
        api::ConfigIdentifier {
            protocol: id.protocol,
            node_type: api::NodeType::from_model(id.node_type) as i32,
            node_version: id.node_version.into(),
        }
    }
}

impl api::ConfigIdentifier {
    /// Parse a `ConfigIdentifier` from a file path.
    ///
    /// The key format looks like:
    /// `/prefix/ethereum/validator/0.0.3/babel.rhai`.
    pub fn from_key<K: AsRef<str>>(key: K) -> Result<Self, Error> {
        let parts: Vec<_> = key.as_ref().split('/').collect();

        let [_prefix, protocol, node_type, node_version, ..] = parts[..] else {
            return Err(Error::SplitKey(key.as_ref().into()));
        };

        let node_type = node_type
            .parse()
            .map(api::NodeType::from_model)
            .map_err(Error::ParseNodeType)?;

        Ok(api::ConfigIdentifier {
            protocol: protocol.to_string(),
            node_type: node_type as i32,
            node_version: node_version.to_string(),
        })
    }
}

impl api::BundleIdentifier {
    /// Extract the bundle version from a key.
    ///
    /// The key format looks like: `0.1.0/bvd-bundle.tgz`
    pub fn from_key<K: AsRef<str>>(key: K) -> Result<Self, Error> {
        let version = key
            .as_ref()
            .strip_suffix(&format!("/{BUNDLE_NAME}"))
            .ok_or_else(|| Error::SuffixBundle(key.as_ref().into()))?;

        let _ =
            Version::parse(version).map_err(|err| Error::ParseVersion(key.as_ref().into(), err))?;

        Ok(api::BundleIdentifier {
            version: version.to_owned(),
        })
    }

    /// Try and parse a `BundleIdentifier` from a key, or return None otherwise.
    pub fn maybe_from_key<K: AsRef<str>>(key: K) -> Option<Self> {
        let key = key.as_ref();
        Self::from_key(key)
            .map_err(|err| trace!("Failed to parse bundle key `{key}`: {err}"))
            .ok()
    }
}

impl api::KernelIdentifier {
    /// Extract the kernel version from a key.
    ///
    /// The key format looks like: `5.10.174-build.1+fc.ufw/kernel.gz`
    pub fn from_key<K: AsRef<str>>(key: K) -> Result<Self, Error> {
        let version = key
            .as_ref()
            .strip_suffix(&format!("/{KERNEL_NAME}"))
            .ok_or_else(|| Error::SuffixKernel(key.as_ref().into()))?;

        Ok(api::KernelIdentifier {
            version: version.to_owned(),
        })
    }

    /// Try and parse a `KernelIdentifier` from a key, or return None otherwise.
    pub fn maybe_from_key<K: AsRef<str>>(key: K) -> Option<Self> {
        let key = key.as_ref();
        Self::from_key(key)
            .map_err(|err| trace!("Failed to parse kernel key `{key}`: {err}"))
            .ok()
    }
}
