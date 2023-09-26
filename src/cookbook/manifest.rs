//! These structures were brutally copy pasted from BlockvisorD repository
//! (blockvisor/babel_api/src/engine.rs).
//!
//! To be removed once switched to monorepo.

use std::path::PathBuf;

use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::grpc::api;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Invalid compression type.
    CompressionType,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileLocation {
    pub path: PathBuf,
    pub pos: u64,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Checksum {
    Sha1([u8; 20]),
    Sha256([u8; 32]),
    Blake3([u8; 32]),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Chunk {
    pub key: String,
    pub url: String,
    pub checksum: Checksum,
    pub size: u64,
    pub destinations: Vec<FileLocation>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Compression {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DownloadManifest {
    pub total_size: u64,
    pub compression: Option<Compression>,
    pub chunks: Vec<Chunk>,
}

impl TryInto<api::Compression> for Compression {
    type Error = Error;

    fn try_into(self) -> Result<api::Compression, Self::Error> {
        Err(Error::CompressionType)
    }
}

impl TryInto<api::DownloadManifest> for DownloadManifest {
    type Error = Error;

    fn try_into(self) -> Result<api::DownloadManifest, Self::Error> {
        let compression: Option<api::Compression> = if let Some(compression) = self.compression {
            Some(compression.try_into()?)
        } else {
            None
        };

        let chunks = self
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

        Ok(api::DownloadManifest {
            total_size: self.total_size,
            compression: compression.map(|value| value.into()),
            chunks,
        })
    }
}
