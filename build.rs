use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

const PROTO_DIR: &str = "proto";

fn main() -> Result<()> {
    #[cfg(any(test, feature = "integration-test"))]
    let builder = tonic_build::configure().build_client(true);
    #[cfg(not(any(test, feature = "integration-test")))]
    let builder = tonic_build::configure();

    builder
        .build_server(true)
        .enum_attribute("command", "#[allow(clippy::large_enum_variant)]")
        .compile(&proto_files()?, &[PROTO_DIR])
        .context("Failed to compile protos")
}

fn proto_files() -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    find_recursive(Path::new(PROTO_DIR), &mut files)?;
    Ok(files)
}

fn find_recursive(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                find_recursive(&path, files)?;
            } else if path.extension().map_or(false, |ext| ext == "proto") {
                files.push(path.strip_prefix(PROTO_DIR)?.to_path_buf());
            }
        }
    }
    Ok(())
}
