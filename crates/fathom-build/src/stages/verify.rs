//! Stage 10 — verify.
//!
//! Round-trip check on a built dist/ tree:
//! 1. Re-read each shard, recompute SHA-256, compare to manifest entry.
//! 2. Re-read the manifest, verify minisig against dist/fathom.pub.
//!
//! Fails loud on any mismatch. Operator runs this before deploy.

use crate::stages::manifest::Manifest;
use crate::stages::shard::dist_dir;
use anyhow::{anyhow, bail, Context, Result};
use minisign::PublicKeyBox;
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub async fn run() -> Result<()> {
    let dist = dist_dir();
    let manifest_path = dist.join("index.msgpack");
    let sig_path = dist.join("index.msgpack.minisig");
    let pub_path = dist.join("fathom.pub");

    let manifest_bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: Manifest = rmp_serde::from_slice(&manifest_bytes)
        .with_context(|| format!("decode {}", manifest_path.display()))?;

    let mut hash_mismatches = 0usize;
    let mut missing = 0usize;
    let shards_dir = dist.join("shards");
    for book in &manifest.books {
        let shard_path = shards_dir.join(&book.shard_filename);
        let bytes = match std::fs::read(&shard_path) {
            Ok(b) => b,
            Err(_) => {
                eprintln!("  missing shard: pg{}", book.gutenberg_id);
                missing += 1;
                continue;
            }
        };
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual = hex_digest(hasher.finalize().as_ref());
        if actual != book.shard_sha256 {
            eprintln!(
                "  HASH MISMATCH pg{}: manifest={} actual={}",
                book.gutenberg_id, book.shard_sha256, actual
            );
            hash_mismatches += 1;
        }
    }

    let pub_string = std::fs::read_to_string(&pub_path)
        .with_context(|| format!("read {}", pub_path.display()))?;
    let pk_box =
        PublicKeyBox::from_string(&pub_string).map_err(|e| anyhow!("parse pub key: {e}"))?;
    let pk = pk_box
        .into_public_key()
        .map_err(|e| anyhow!("decode pub key: {e}"))?;

    let sig_string = std::fs::read_to_string(&sig_path)
        .with_context(|| format!("read {}", sig_path.display()))?;
    let sig_box = minisign::SignatureBox::from_string(&sig_string)
        .map_err(|e| anyhow!("parse signature: {e}"))?;

    let mut reader = Cursor::new(&manifest_bytes);
    minisign::verify(&pk, &sig_box, &mut reader, true, false, false)
        .map_err(|e| anyhow!("signature verify failed: {e}"))?;

    if hash_mismatches > 0 || missing > 0 {
        bail!(
            "verify failed: {} hash mismatches, {} missing shards",
            hash_mismatches,
            missing
        );
    }

    eprintln!(
        "verify: OK — manifest signed correctly, {} shards hash-match",
        manifest.book_count
    );
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
