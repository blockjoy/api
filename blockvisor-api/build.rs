use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

// multiple paths lets Dockerfile find them
const PROTO_DIRS: &[&str] = &["./proto", "../proto"];
const EXCLUDE_DIRS: &[&str] = &[".direnv"];

fn main() -> Result<()> {
    #[cfg(any(test, feature = "integration-test"))]
    let builder = tonic_build::configure().build_client(true);
    #[cfg(not(any(test, feature = "integration-test")))]
    let builder = tonic_build::configure();

    builder
        .build_server(true)
        .enum_attribute("command", "#[allow(clippy::large_enum_variant)]")
        .compile(&proto_files()?, PROTO_DIRS)
        .context("Failed to compile protos")
}

fn proto_files() -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for dir in PROTO_DIRS {
        find_recursive(Path::new(dir), &mut files)?;
    }
    Ok(files)
}

fn find_recursive(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let is_excluded = || {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| EXCLUDE_DIRS.contains(&name))
            .unwrap_or_default()
    };

    if !path.is_dir() || is_excluded() {
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            find_recursive(&path, files)?;
        } else if path.extension().map_or(false, |ext| ext == "proto") {
            files.push(path.to_path_buf());
        }
    }

    Ok(())
}
