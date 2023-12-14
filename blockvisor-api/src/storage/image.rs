use displaydoc::Display;
use semver::Version;
use thiserror::Error;
use tracing::trace;

use crate::grpc::{api, common};
use crate::models::node::{NodeType, NodeVersion};

use super::{BUNDLE_FILE, KERNEL_FILE};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse NodeVersion from key `{0}`: {1}
    ParseKeyVersion(String, crate::models::node::node_type::Error),
    /// Failed to parse NodeType from key `{0}`: {1}
    ParseNodeType(String, crate::models::node::node_type::Error),
    /// Failed to parse semantic version from NodeVersion `{0}`: {1}
    ParseSemver(NodeVersion, crate::models::node::node_type::Error),
    /// Failed to parse NodeVersion: {0}
    ParseVersion(crate::models::node::node_type::Error),
    /// Key `{0}` is not splittable into at least 4 `/` separated parts.
    SplitKey(String),
    /// File name should end in `/{BUNDLE_FILE:?}` but is `{0}`.
    SuffixBundle(String),
    /// File name should end in `/{KERNEL_FILE:?}` but is `{0}`.
    SuffixKernel(String),
}

/// A storage image identifier.
#[derive(Clone, Debug)]
pub struct ImageId {
    pub protocol: String,
    pub node_type: NodeType,
    pub node_version: NodeVersion,
}

impl ImageId {
    pub fn new<S: Into<String>>(
        protocol: S,
        node_type: NodeType,
        node_version: NodeVersion,
    ) -> Self {
        ImageId {
            protocol: protocol.into(),
            node_type,
            node_version,
        }
    }

    /// Parse an `ImageId` from a key.
    ///
    /// Example key format: `/prefix/ethereum/validator/0.0.3/babel.rhai`.
    pub fn from_key<K: AsRef<str>>(key: K) -> Result<Self, Error> {
        let key = key.as_ref();
        let parts: Vec<_> = key.split('/').collect();

        let [_prefix, protocol, node_type, node_version, ..] = parts[..] else {
            return Err(Error::SplitKey(key.into()));
        };

        let node_type = node_type
            .parse::<NodeType>()
            .map_err(|err| Error::ParseNodeType(key.into(), err))?;
        let node_version = NodeVersion::new(node_version)
            .map_err(|err| Error::ParseKeyVersion(key.into(), err))?;

        Ok(ImageId {
            protocol: protocol.to_string(),
            node_type,
            node_version,
        })
    }

    pub fn key(&self, prefix: &str, file: &str) -> String {
        format!(
            "{prefix}/{protocol}/{node_type}/{node_version}/{file}",
            protocol = self.protocol,
            node_type = self.node_type,
            node_version = self.node_version
        )
    }

    pub fn semver(&self) -> Result<Version, Error> {
        self.node_version
            .semver()
            .map_err(|err| Error::ParseSemver(self.node_version.clone(), err))
    }
}

impl From<ImageId> for common::ImageIdentifier {
    fn from(image: ImageId) -> Self {
        common::ImageIdentifier {
            protocol: image.protocol,
            node_type: common::NodeType::from(image.node_type).into(),
            node_version: image.node_version.to_string(),
        }
    }
}

impl TryFrom<common::ImageIdentifier> for ImageId {
    type Error = Error;

    fn try_from(image: common::ImageIdentifier) -> Result<Self, Self::Error> {
        let node_type = image.node_type().into();
        let node_version = NodeVersion::new(&image.node_version).map_err(Error::ParseVersion)?;

        Ok(ImageId {
            protocol: image.protocol,
            node_type,
            node_version,
        })
    }
}

impl api::BundleIdentifier {
    /// Extract the bundle version from a key.
    ///
    /// Example key format: `0.1.0/bvd-bundle.tgz`
    pub fn from_key<K: AsRef<str>>(key: K) -> Result<Self, Error> {
        let key = key.as_ref();
        let version = key
            .strip_suffix(&format!("/{BUNDLE_FILE}"))
            .ok_or_else(|| Error::SuffixBundle(key.into()))?;

        let _ = NodeVersion::new(version).map_err(|err| Error::ParseKeyVersion(key.into(), err))?;

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
    /// Example key format: `5.10.174-build.1+fc.ufw/kernel.gz`
    pub fn from_key<K: AsRef<str>>(key: K) -> Result<Self, Error> {
        let key = key.as_ref();
        let version = key
            .strip_suffix(&format!("/{KERNEL_FILE}"))
            .ok_or_else(|| Error::SuffixKernel(key.into()))?;

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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_image_from_key() {
        ImageId::from_key("chains/testing/validator/0.0.1").unwrap();
        ImageId::from_key("chains/testing/validator/0.0.1/babel.rhai").unwrap();
    }

    #[test]
    fn test_bundles_from_key() {
        let tests = [
            ("/bvd-bundle.tgz", false),
            ("0.0.0/tester.txt", false),
            ("0.1.0/bvd-bundle.tgz", true),
            ("0.10.0/bvd-bundle.tgz", true),
        ];

        for (test, pass) in tests {
            let result = api::BundleIdentifier::from_key(test);
            if pass {
                assert!(result.is_ok());
            } else {
                assert!(result.is_err());
            }
        }
    }
}
