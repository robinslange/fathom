//! Build state directory layout.
//!
//! Each stage writes to a stable path under `build-state/`. Stages are
//! idempotent: re-runs overwrite outputs but don't re-fetch unchanged inputs
//! (where caching applies).

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub fn build_state_dir() -> PathBuf {
    std::env::var("FATHOM_BUILD_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("build-state"))
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).with_context(|| format!("create dir {}", path.display()))
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let f = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    serde_json::to_writer_pretty(std::io::BufWriter::new(f), value)
        .with_context(|| format!("write json to {}", path.display()))
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let f = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    serde_json::from_reader(std::io::BufReader::new(f))
        .with_context(|| format!("parse json from {}", path.display()))
}

pub fn candidates_path() -> PathBuf {
    build_state_dir().join("candidates.json")
}

pub fn translators_path() -> PathBuf {
    build_state_dir().join("translators.json")
}

pub fn filtered_path() -> PathBuf {
    build_state_dir().join("filtered.json")
}

pub fn catalog_csv_path() -> PathBuf {
    build_state_dir().join("pg_catalog.csv")
}

pub fn rdf_cache_dir() -> PathBuf {
    build_state_dir().join("rdf-cache")
}
