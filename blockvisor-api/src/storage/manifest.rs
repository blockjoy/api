use std::path::PathBuf;

use displaydoc::Display;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use url::Url;

use crate::grpc::api;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Unexpected Blake3 checksum bytes: {0:x?}
    ChecksumBlake3(Vec<u8>),
    /// Unexpected SHA1 checksum bytes: {0:x?}
    ChecksumSha1(Vec<u8>),
    /// Unexpected SHA256 checksum bytes: {0:x?}
    ChecksumSha256(Vec<u8>),
    /// Missing Checksum.
    MissingChecksum,
    /// Missing Compression type.
    MissingCompression,
    /// Failed to parse ArchiveChunk URL: {0}
    ParseArchiveUrl(url::ParseError),
    /// Failed to parse upload URL: {0}
    ParseUploadUrl(url::ParseError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadManifest {
    pub total_size: u64,
    pub compression: Option<Compression>,
    pub chunks: Vec<ArchiveChunk>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveChunk {
    pub key: String,
    #[serde(deserialize_with = "deserialize_option_url")]
    pub url: Option<Url>,
    pub checksum: Checksum,
    pub size: u64,
    pub destinations: Vec<ChunkTarget>,
}

impl TryFrom<api::ArchiveChunk> for ArchiveChunk {
    type Error = Error;

    fn try_from(chunk: api::ArchiveChunk) -> Result<Self, Self::Error> {
        Ok(ArchiveChunk {
            key: chunk.key,
            url: chunk
                .url
                .map(|url| url.parse().map_err(Error::ParseArchiveUrl))
                .transpose()?,
            checksum: chunk.checksum.ok_or(Error::MissingChecksum)?.try_into()?,
            size: chunk.size,
            destinations: chunk.destinations.into_iter().map(Into::into).collect(),
        })
    }
}

impl From<ArchiveChunk> for api::ArchiveChunk {
    fn from(chunk: ArchiveChunk) -> Self {
        api::ArchiveChunk {
            key: chunk.key,
            url: chunk.url.map(|url| url.to_string()),
            checksum: Some(chunk.checksum.into()),
            size: chunk.size,
            destinations: chunk.destinations.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkTarget {
    pub path: PathBuf,
    #[serde(alias = "pos")]
    pub position: u64,
    pub size: u64,
}

impl From<api::ChunkTarget> for ChunkTarget {
    fn from(target: api::ChunkTarget) -> Self {
        ChunkTarget {
            path: target.path.into(),
            position: target.position_bytes,
            size: target.size_bytes,
        }
    }
}

impl From<ChunkTarget> for api::ChunkTarget {
    fn from(target: ChunkTarget) -> Self {
        api::ChunkTarget {
            path: target.path.to_string_lossy().to_string(),
            position_bytes: target.position,
            size_bytes: target.size,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Checksum {
    Sha1([u8; 20]),
    Sha256([u8; 32]),
    Blake3([u8; 32]),
}

impl TryFrom<api::Checksum> for Checksum {
    type Error = Error;

    fn try_from(checksum: api::Checksum) -> Result<Self, Self::Error> {
        match checksum.checksum.ok_or(Error::MissingChecksum)? {
            api::checksum::Checksum::Sha1(bytes) => Ok(Checksum::Sha1(
                bytes.try_into().map_err(Error::ChecksumSha1)?,
            )),
            api::checksum::Checksum::Sha256(bytes) => Ok(Checksum::Sha256(
                bytes.try_into().map_err(Error::ChecksumSha256)?,
            )),
            api::checksum::Checksum::Blake3(bytes) => Ok(Checksum::Blake3(
                bytes.try_into().map_err(Error::ChecksumBlake3)?,
            )),
        }
    }
}

impl From<Checksum> for api::Checksum {
    fn from(checksum: Checksum) -> Self {
        let inner = match checksum {
            Checksum::Sha1(bytes) => api::checksum::Checksum::Sha1(bytes.to_vec()),
            Checksum::Sha256(bytes) => api::checksum::Checksum::Sha256(bytes.to_vec()),
            Checksum::Blake3(bytes) => api::checksum::Checksum::Blake3(bytes.to_vec()),
        };

        api::Checksum {
            checksum: Some(inner),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Compression {
    ZStd(i32),
}

impl TryFrom<api::Compression> for Compression {
    type Error = Error;

    fn try_from(compression: api::Compression) -> Result<Self, Self::Error> {
        match compression.compression.ok_or(Error::MissingCompression)? {
            api::compression::Compression::Zstd(level) => Ok(Compression::ZStd(level)),
        }
    }
}

impl From<Compression> for api::Compression {
    fn from(compression: Compression) -> Self {
        let inner = match compression {
            Compression::ZStd(level) => api::compression::Compression::Zstd(level),
        };

        api::Compression {
            compression: Some(inner),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadSlot {
    pub key: String,
    pub url: Url,
}

impl TryFrom<api::UploadSlot> for UploadSlot {
    type Error = Error;

    fn try_from(slot: api::UploadSlot) -> Result<Self, Self::Error> {
        Ok(UploadSlot {
            key: slot.key,
            url: slot.url.parse().map_err(Error::ParseUploadUrl)?,
        })
    }
}

impl From<UploadSlot> for api::UploadSlot {
    fn from(slot: UploadSlot) -> Self {
        api::UploadSlot {
            key: slot.key,
            url: slot.url.to_string(),
        }
    }
}

fn deserialize_option_url<'de, D>(deserializer: D) -> Result<Option<Url>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(ref s) if s.is_empty() => Ok(None),
        Some(s) => s.parse().map(Some).map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_archive_chunk() {
        let json = r#"{
            "key": "some_chunk",
            "url": "http://some.url",
            "checksum": {
                "sha256": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
            },
            "size": 123,
            "destinations": []
        }"#;

        let _: ArchiveChunk = serde_json::from_str(json).unwrap();
    }

    #[test]
    fn parse_chunk_empty_url() {
        let json = r#"{
            "key": "some_chunk",
            "url": "",
            "checksum": {
                "sha256": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
            },
            "size": 123,
            "destinations": []
        }"#;

        let _: ArchiveChunk = serde_json::from_str(json).unwrap();
    }
}
