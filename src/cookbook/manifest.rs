//! Structs copied from `blockvisor/babel_api/src/engine.rs`.
//!
//! Should be removed if we switch to a monorepo.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::grpc::api;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLocation {
    pub path: PathBuf,
    pub pos: u64,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Checksum {
    Sha1([u8; 20]),
    Sha256([u8; 32]),
    Blake3([u8; 32]),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    pub key: String,
    pub url: String,
    pub checksum: Checksum,
    pub size: u64,
    pub destinations: Vec<FileLocation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Compression {
    ZSTD(i32),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadManifest {
    pub total_size: u64,
    pub compression: Option<Compression>,
    pub chunks: Vec<Chunk>,
}

impl From<Compression> for api::compression::Compression {
    fn from(value: Compression) -> Self {
        match value {
            Compression::ZSTD(level) => api::compression::Compression::Zstd(level),
        }
    }
}

impl From<DownloadManifest> for api::DownloadManifest {
    fn from(value: DownloadManifest) -> Self {
        let compression: Option<api::Compression> =
            value.compression.map(|compression| api::Compression {
                compression: Some(compression.into()),
            });

        let chunks = value
            .chunks
            .into_iter()
            .map(|value| {
                let (checksum_type, checksum) = match value.checksum {
                    Checksum::Sha1(value) => (api::ChecksumType::Sha1, value.to_vec()),
                    Checksum::Sha256(value) => (api::ChecksumType::Sha256, value.to_vec()),
                    Checksum::Blake3(value) => (api::ChecksumType::Blake3, value.to_vec()),
                };

                let destinations = value
                    .destinations
                    .into_iter()
                    .map(|value| api::FileLocation {
                        path: value.path.to_string_lossy().to_string(),
                        position_bytes: value.pos,
                        size_bytes: value.size,
                    })
                    .collect();

                api::Chunk {
                    key: value.key,
                    url: value.url,
                    checksum_type: checksum_type.into(),
                    checksum,
                    size: value.size,
                    destinations,
                }
            })
            .collect();

        Self {
            total_size: value.total_size,
            compression,
            chunks,
        }
    }
}
