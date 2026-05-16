//! Stage 11 — deploy.
//!
//! Operator-only stage. Uploads the signed dist tree to R2 via the locally
//! authenticated `wrangler` CLI. Atomic ordering: shards first (idempotent;
//! content-addressed by SHA in the filename), then `fathom.pub`, then the
//! manifest signature `.minisig`, finally `index.msgpack` itself. A client
//! racing the deploy sees either the old or the new manifest — never a
//! manifest pointing at not-yet-uploaded shards.
//!
//! Requires `wrangler` (npm-installed, OAuth-authenticated for the same
//! Cloudflare account that owns the target bucket).

use crate::stages::shard::dist_dir;
use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_BUCKET: &str = "fathom-corpus";

/// Shards are content-addressed by SHA in their filename and the manifest's
/// shard_sha256 field. Once uploaded under a given SHA they never change, so
/// the edge can cache them effectively forever.
const SHARD_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// Manifest + signature change per deploy. Short TTL so users see updates
/// quickly; must-revalidate forces the edge to re-check before serving stale.
const MANIFEST_CACHE_CONTROL: &str = "public, max-age=60, must-revalidate";

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    /// R2 bucket name. Defaults to `fathom-corpus`.
    #[arg(long, default_value = DEFAULT_BUCKET)]
    pub bucket: String,
    /// Skip the shard upload pass — useful when only the manifest changed
    /// (e.g. tradition tagging updates without a corpus re-embed).
    #[arg(long)]
    pub skip_shards: bool,
    /// Override the path to the wrangler binary. Defaults to whatever's on PATH.
    #[arg(long, default_value = "wrangler")]
    pub wrangler: String,
    /// Upload only the first N shards — useful for smoke-testing the deploy
    /// path without burning the full corpus's bandwidth.
    #[arg(long)]
    pub limit: Option<usize>,
}

pub async fn run(args: Args) -> Result<()> {
    let dist = dist_dir();
    let manifest_path = dist.join("index.msgpack");
    let sig_path = dist.join("index.msgpack.minisig");
    let pub_path = dist.join("fathom.pub");
    let shards_dir = dist.join("shards");

    for p in [&manifest_path, &sig_path, &pub_path] {
        if !p.is_file() {
            bail!(
                "{} not found — run `fathom-build all` first",
                p.display()
            );
        }
    }
    if !shards_dir.is_dir() {
        bail!(
            "shards dir {} missing — run `fathom-build shard` first",
            shards_dir.display()
        );
    }

    if !args.skip_shards {
        upload_shards(&args, &shards_dir)?;
    }
    upload_object(&args, &pub_path, "fathom.pub", "application/octet-stream", MANIFEST_CACHE_CONTROL)?;
    upload_object(&args, &sig_path, "index.msgpack.minisig", "application/octet-stream", MANIFEST_CACHE_CONTROL)?;
    upload_object(&args, &manifest_path, "index.msgpack", "application/msgpack", MANIFEST_CACHE_CONTROL)?;

    eprintln!("deploy: complete");
    Ok(())
}

fn upload_shards(args: &Args, shards_dir: &Path) -> Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(shards_dir)
        .with_context(|| format!("read shards dir {}", shards_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|s| s == "shard"))
        .collect();
    entries.sort();
    if let Some(n) = args.limit {
        entries.truncate(n);
    }

    eprintln!("deploy: uploading {} shards", entries.len());
    for (i, path) in entries.iter().enumerate() {
        let filename = path
            .file_name()
            .ok_or_else(|| anyhow!("shard path has no filename: {}", path.display()))?
            .to_string_lossy()
            .into_owned();
        let key = format!("shards/{filename}");
        upload_object(args, path, &key, "application/octet-stream", SHARD_CACHE_CONTROL)?;
        if (i + 1) % 50 == 0 || i + 1 == entries.len() {
            eprintln!("  ...{}/{}", i + 1, entries.len());
        }
    }
    Ok(())
}

fn upload_object(
    args: &Args,
    local_path: &Path,
    remote_key: &str,
    content_type: &str,
    cache_control: &str,
) -> Result<()> {
    let object_path = format!("{}/{}", args.bucket, remote_key);
    let status = Command::new(&args.wrangler)
        .arg("r2")
        .arg("object")
        .arg("put")
        .arg(&object_path)
        .arg("--file")
        .arg(local_path)
        .arg("--content-type")
        .arg(content_type)
        .arg("--cache-control")
        .arg(cache_control)
        .arg("--remote")
        .status()
        .with_context(|| format!("spawn wrangler r2 object put {object_path}"))?;
    if !status.success() {
        bail!(
            "wrangler r2 object put {} failed with status {}",
            object_path, status
        );
    }
    Ok(())
}
