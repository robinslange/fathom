//! Library runtime: manifest fetch + verify, lazy shard cache, kNN search.
//!
//! On Apple platforms loads via `fathom_embed` (CPU bge-small). The runtime is
//! offline-by-default after first launch: the manifest is cached locally and
//! re-verified on every load; shards are fetched on demand and persisted.
//!
//! See `docs/superpowers/specs/2026-05-16-fathom-v0.2-runtime-swap-design.md`
//! for the architectural shape this serves.

use anyhow::{anyhow, bail, Context, Result};
use directories::ProjectDirs;
use lru::LruCache;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

/// Public verifying key for the corpus manifest. Generated alongside the
/// fathom-build sign stage; the bytes are baked into the binary.
const FATHOM_PUB: &[u8] = include_bytes!("../data/fathom.pub");

/// Default base URL for manifest + shard fetch. Override via `FATHOM_MANIFEST_URL`.
const DEFAULT_BASE_URL: &str = "https://corpus.fathom.omit.nz";

/// Shard format version this runtime understands. Must match the build-time constant.
/// Shards with a different version are rejected at decode time.
pub const SHARD_FORMAT_VERSION: u32 = 2;

/// Manifest schema as written by `fathom-build manifest`. Wire-compat with
/// `crates/fathom-build/src/stages/manifest.rs::Manifest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest_version: u32,
    pub build_id: String,
    pub generated: String,
    pub embed_model_id: String,
    pub embed_dims: usize,
    pub book_count: usize,
    pub books: Vec<ManifestBook>,
}

/// Per-book entry in the manifest. Wire-compat with
/// `crates/fathom-build/src/stages/manifest.rs::ManifestBook`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestBook {
    pub gutenberg_id: u32,
    pub title: String,
    /// Translator agents — Agent type lives in fathom-build; we mirror the
    /// minimal subset (name + life dates) here so fathom-core doesn't depend
    /// on fathom-build's types.
    pub translators: Vec<TranslatorEntry>,
    pub locc: Vec<String>,
    pub tradition: String,
    pub shard_filename: String,
    pub shard_sha256: String,
    pub shard_size_bytes: u64,
    pub chunk_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslatorEntry {
    pub name: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
}

pub fn cache_root() -> Result<PathBuf> {
    let proj = ProjectDirs::from("nz", "omit", "fathom")
        .ok_or_else(|| anyhow!("could not resolve OS project directories"))?;
    Ok(proj.data_dir().join("corpus"))
}

pub fn manifest_path() -> Result<PathBuf> {
    Ok(cache_root()?.join("index.msgpack"))
}

pub fn shard_path(filename: &str) -> Result<PathBuf> {
    Ok(cache_root()?.join("shards").join(filename))
}

/// Base URL for manifest + shard fetch. Reads `$FATHOM_MANIFEST_URL` if set,
/// otherwise the in-binary default.
pub fn base_url() -> String {
    std::env::var("FATHOM_MANIFEST_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

/// Fetch the manifest from the network, verify its signature, write to cache.
///
/// Returns the decoded `Manifest`. Steps:
/// 1. GET `<base>/index.msgpack` and `<base>/index.msgpack.minisig` to .partial files.
/// 2. Verify the signature against the baked-in `fathom.pub`.
/// 3. Atomic-rename .partial → final paths.
/// 4. Decode the msgpack.
///
/// If the network is offline but a cached manifest exists, falls back to the
/// cache (without re-verifying — the file got verified the last time we
/// fetched it). Returns `Err` if neither network nor cache yields a manifest.
pub async fn fetch_manifest() -> Result<Manifest> {
    let base = base_url();
    let mp_url = format!("{base}/index.msgpack");
    let sig_url = format!("{base}/index.msgpack.minisig");

    let cache_dir = cache_root()?;
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .with_context(|| format!("create cache dir {}", cache_dir.display()))?;
    let mp_path = manifest_path()?;
    let sig_path = cache_dir.join("index.msgpack.minisig");
    let mp_partial = mp_path.with_extension("msgpack.partial");
    let sig_partial = sig_path.with_extension("minisig.partial");

    let client = reqwest::Client::builder()
        .user_agent(concat!("fathom/", env!("CARGO_PKG_VERSION")))
        .build()?;

    match (
        download_to(&client, &mp_url, &mp_partial).await,
        download_to(&client, &sig_url, &sig_partial).await,
    ) {
        (Ok(()), Ok(())) => {
            verify_signature(&mp_partial, &sig_partial)?;
            tokio::fs::rename(&mp_partial, &mp_path)
                .await
                .with_context(|| format!("finalise {}", mp_path.display()))?;
            tokio::fs::rename(&sig_partial, &sig_path)
                .await
                .with_context(|| format!("finalise {}", sig_path.display()))?;
        }
        _ => {
            // Network failed. Fall back to cache if it exists.
            if !mp_path.is_file() {
                let _ = tokio::fs::remove_file(&mp_partial).await;
                let _ = tokio::fs::remove_file(&sig_partial).await;
                bail!(
                    "manifest fetch failed and no cached manifest at {}",
                    mp_path.display()
                );
            }
        }
    }

    let bytes = tokio::fs::read(&mp_path)
        .await
        .with_context(|| format!("read manifest {}", mp_path.display()))?;
    let manifest: Manifest = rmp_serde::from_slice(&bytes).context("decode manifest msgpack")?;
    Ok(manifest)
}

async fn download_to(client: &reqwest::Client, url: &str, dest: &Path) -> Result<()> {
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()?;
    let bytes = resp.bytes().await.with_context(|| format!("body {url}"))?;
    tokio::fs::write(dest, &bytes)
        .await
        .with_context(|| format!("write {}", dest.display()))?;
    Ok(())
}

fn verify_signature(manifest_path: &Path, sig_path: &Path) -> Result<()> {
    use minisign_verify::{PublicKey, Signature};
    let pubkey = PublicKey::from_base64(
        std::str::from_utf8(FATHOM_PUB)
            .context("fathom.pub not utf-8")?
            .lines()
            .find(|l| !l.starts_with("untrusted comment:") && !l.is_empty())
            .ok_or_else(|| anyhow!("no key line in fathom.pub"))?,
    )
    .map_err(|e| anyhow!("parse fathom.pub: {e}"))?;
    let sig_bytes =
        std::fs::read(sig_path).with_context(|| format!("read sig {}", sig_path.display()))?;
    let sig = Signature::decode(std::str::from_utf8(&sig_bytes).context("sig not utf-8")?)
        .map_err(|e| anyhow!("parse signature: {e}"))?;
    let manifest_bytes = std::fs::read(manifest_path)
        .with_context(|| format!("read manifest for verify {}", manifest_path.display()))?;
    pubkey
        .verify(&manifest_bytes, &sig, false)
        .map_err(|e| anyhow!("manifest signature invalid: {e}"))?;
    Ok(())
}

/// Shard schema as written by `fathom-build shard`. Wire-compat with
/// `crates/fathom-build/src/shard_format.rs::Shard`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shard {
    pub format_version: u32,
    pub gutenberg_id: u32,
    pub title: String,
    pub translators: Vec<TranslatorEntry>,
    pub embed_model_id: String,
    pub canonical_text: String,
    pub chunks: Vec<ShardChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardChunk {
    pub chunk_id: String,
    pub paragraph_id: String,
    pub section_id: Option<String>,
    /// UTF-8 byte offset into `canonical_text`.
    pub byte_offset_start: usize,
    pub byte_offset_end: usize,
    pub token_count: usize,
    #[serde(with = "serde_bytes")]
    pub embedding_f16: Vec<u8>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn decode_shard(raw: &[u8]) -> Result<Shard> {
    let decompressed = zstd::decode_all(raw).context("zstd decode shard")?;
    let shard: Shard = rmp_serde::from_slice(&decompressed).context("decode shard msgpack")?;
    if shard.format_version != SHARD_FORMAT_VERSION {
        anyhow::bail!(
            "shard format_version {} != runtime {}",
            shard.format_version,
            SHARD_FORMAT_VERSION
        );
    }
    Ok(shard)
}

/// Maximum number of decoded shards to keep in memory. 64 covers most search
/// + read-multiple-books sessions without unbounded growth.
const SHARD_CACHE_CAPACITY: usize = 64;

/// Live runtime: holds the verified manifest and an LRU cache of decoded
/// shards. Construct via `Runtime::new()` after `fetch_manifest()`.
pub struct Runtime {
    manifest: Manifest,
    shards: AsyncMutex<LruCache<u32, Arc<Shard>>>,
    http: reqwest::Client,
}

impl Runtime {
    pub fn new(manifest: Manifest) -> Self {
        Self::with_cache_capacity(manifest, SHARD_CACHE_CAPACITY)
    }

    /// Same as `new` but allows overriding the LRU cache capacity. Useful for
    /// benchmarks or other "load everything" callers that need the entire
    /// corpus searchable in memory rather than the desktop default of 64.
    pub fn with_cache_capacity(manifest: Manifest, capacity: usize) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("fathom/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client builds");
        let cap = NonZeroUsize::new(capacity.max(1)).expect("capacity max(1) > 0");
        Self {
            manifest,
            shards: AsyncMutex::new(LruCache::new(cap)),
            http,
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Look up a book entry by gutenberg_id.
    pub fn book(&self, gutenberg_id: u32) -> Option<&ManifestBook> {
        self.manifest
            .books
            .iter()
            .find(|b| b.gutenberg_id == gutenberg_id)
    }

    /// Ensure the shard for `gutenberg_id` is cached locally and decoded.
    /// Network on cache miss, file IO on disk-cache hit, no-op on memory hit.
    ///
    /// Retries once if the disk-cached file is stale relative to the manifest
    /// (manifest updated since last fetch). The first attempt deletes the stale
    /// file and the second attempt triggers the cache-miss network path.
    pub async fn ensure_shard(&self, gutenberg_id: u32) -> Result<Arc<Shard>> {
        if let Some(s) = self.shards.lock().await.get(&gutenberg_id) {
            return Ok(s.clone());
        }
        let book = self
            .book(gutenberg_id)
            .ok_or_else(|| anyhow!("unknown gutenberg_id {gutenberg_id}"))?
            .clone();
        let local = shard_path(&book.shard_filename)?;

        // Capture the verified bytes from whichever branch produces them so
        // decode_shard below doesn't have to re-read the file. Network branch
        // already has them in memory; disk-hit branch reads once and reuses.
        let mut raw: Option<Vec<u8>> = None;
        for attempt in 0..2u8 {
            if !local.is_file() {
                let url = format!("{}/shards/{}", base_url(), book.shard_filename);
                let bytes = self
                    .http
                    .get(&url)
                    .send()
                    .await
                    .with_context(|| format!("GET {url}"))?
                    .error_for_status()?
                    .bytes()
                    .await?;
                let observed = sha256_hex(&bytes);
                if !observed.eq_ignore_ascii_case(&book.shard_sha256) {
                    bail!(
                        "shard {} sha256 mismatch: expected {} got {}",
                        book.shard_filename,
                        book.shard_sha256,
                        observed
                    );
                }
                if let Some(parent) = local.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .with_context(|| format!("create {}", parent.display()))?;
                }
                let partial = local.with_extension("shard.partial");
                tokio::fs::write(&partial, &bytes)
                    .await
                    .with_context(|| format!("write {}", partial.display()))?;
                tokio::fs::rename(&partial, &local)
                    .await
                    .with_context(|| format!("finalise {}", local.display()))?;
                raw = Some(bytes.to_vec());
            } else {
                let bytes = tokio::fs::read(&local).await?;
                if !sha256_hex(&bytes).eq_ignore_ascii_case(&book.shard_sha256) {
                    tokio::fs::remove_file(&local).await.ok();
                    if attempt == 0 {
                        continue;
                    }
                    bail!(
                        "shard {} stale after refetch; manifest may be inconsistent",
                        local.display()
                    );
                }
                raw = Some(bytes);
            }
            break;
        }

        let raw = raw.expect("loop above must populate raw or bail");
        let shard = match decode_shard(&raw) {
            Ok(s) => s,
            Err(e) => {
                tokio::fs::remove_file(&local).await.ok();
                return Err(e);
            }
        };
        let arc = Arc::new(shard);
        self.shards.lock().await.put(gutenberg_id, arc.clone());
        Ok(arc)
    }

    /// Embed `query` via fathom-embed and rank chunks across all shards
    /// currently in the LRU cache. Cold cache → empty result. Caller-driven:
    /// the desktop UI eagerly `load_book`s the manifest's first N books on
    /// startup, then re-searches as the user reads.
    pub async fn search(&self, query: &str, top_n: usize) -> Result<Vec<SearchHit>> {
        let q = fathom_embed::embed(query).context("embed query")?;
        // Snapshot Arc<Shard> handles out of the LRU and release the lock
        // before the kNN scan. The scan is the bulk of the work; holding the
        // cache mutex across it would block every concurrent ensure_shard /
        // search call during a prewarm burst.
        let shards: Vec<Arc<Shard>> = {
            let cache = self.shards.lock().await;
            cache.iter().map(|(_, s)| s.clone()).collect()
        };
        let mut hits: Vec<SearchHit> = Vec::new();
        for shard in &shards {
            for chunk in &shard.chunks {
                let v = fathom_embed::from_f16_bytes(&chunk.embedding_f16);
                let sim = cosine(&q.vector, &v);
                hits.push(SearchHit {
                    gutenberg_id: shard.gutenberg_id,
                    chunk_id: chunk.chunk_id.clone(),
                    excerpt: chunk_excerpt(&shard.canonical_text, chunk),
                    similarity: sim,
                });
            }
        }
        hits.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.gutenberg_id.cmp(&b.gutenberg_id))
                .then_with(|| a.chunk_id.cmp(&b.chunk_id))
        });
        hits.truncate(top_n);
        Ok(hits)
    }

    /// Snap a document-absolute byte-offset selection (which may span chunks)
    /// to the enclosing UAX#29 sentence boundaries within `canonical_text`.
    ///
    /// If the UAX#29 snap fails (selection lies between sentence spans, or
    /// `unicode_sentences` segmented around a feature like an em-dash without
    /// classifying byte 0 as a sentence start), falls back to the raw selection
    /// clamped to UTF-8 char boundaries. We'd rather paraphrase the user's
    /// literal selection than silently fail; quality cost is a possibly
    /// mid-sentence cut, robustness gain is the user always gets something.
    ///
    /// Returns `Ok(None)` only when the selection collapses to zero length.
    pub async fn snap_selection(
        &self,
        gutenberg_id: u32,
        start_byte: usize,
        end_byte: usize,
    ) -> Result<Option<(usize, usize)>> {
        let shard = self.ensure_shard(gutenberg_id).await?;
        let text = &shard.canonical_text;
        let clamped_start = start_byte.min(text.len());
        let clamped_end = end_byte.min(text.len());
        if clamped_end <= clamped_start {
            return Ok(None);
        }
        // Try UAX#29 sentence snap first.
        if let Some(snapped) = fathom_chunker::snap_to_sentence(text, clamped_start, clamped_end) {
            return Ok(Some(snapped));
        }
        // Fallback: walk to nearest char boundary in/out.
        let mut s = clamped_start;
        while s < text.len() && !text.is_char_boundary(s) {
            s += 1;
        }
        let mut e = clamped_end;
        while e > 0 && e < text.len() && !text.is_char_boundary(e) {
            e -= 1;
        }
        if e <= s {
            return Ok(None);
        }
        Ok(Some((s, e)))
    }
}

/// Search hit: book + chunk + similarity score + an excerpt for previewing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub gutenberg_id: u32,
    pub chunk_id: String,
    pub excerpt: String,
    pub similarity: f32,
}

/// Cosine similarity of two equal-length f32 vectors. bge-small outputs are
/// already L2-normalised so this is dot-product; kept as a separate helper
/// in case downstream callers feed unnormalised vectors.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-12);
    dot / denom
}

fn chunk_excerpt(canonical: &str, chunk: &ShardChunk) -> String {
    const MAX: usize = 200;
    let slice = &canonical[chunk.byte_offset_start..chunk.byte_offset_end];
    if slice.len() <= MAX {
        slice.to_string()
    } else {
        // Truncate at a char boundary.
        let mut end = MAX;
        while end < slice.len() && !slice.is_char_boundary(end) {
            end += 1;
        }
        let cut = &slice[..end];
        let ellipsis_at = cut.rfind(' ').unwrap_or(end);
        format!("{}…", &slice[..ellipsis_at])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_root_under_project_dirs() {
        let root = cache_root().expect("project dirs resolvable on test host");
        assert!(root.ends_with("corpus"));
        assert!(root.to_string_lossy().contains("fathom"));
    }

    #[test]
    fn shard_path_includes_filename() {
        let p = shard_path("123.shard").expect("project dirs resolvable");
        assert!(p.to_string_lossy().ends_with("/shards/123.shard"));
    }

    #[test]
    fn manifest_msgpack_roundtrip() {
        let m = Manifest {
            manifest_version: 1,
            build_id: "2026-05".into(),
            generated: "2026-05-16T22:30:00Z".into(),
            embed_model_id: "bge-small-en-v1.5".into(),
            embed_dims: 384,
            book_count: 1,
            books: vec![ManifestBook {
                gutenberg_id: 45109,
                title: "Enchiridion".into(),
                translators: vec![TranslatorEntry {
                    name: "Long, George".into(),
                    birth_year: Some(1800),
                    death_year: Some(1879),
                }],
                locc: vec!["B".into()],
                tradition: "Stoic".into(),
                shard_filename: "45109.shard".into(),
                shard_sha256: "0".repeat(64),
                shard_size_bytes: 12345,
                chunk_count: 17,
            }],
        };
        let bytes = rmp_serde::to_vec_named(&m).expect("encode");
        let back: Manifest = rmp_serde::from_slice(&bytes).expect("decode");
        assert_eq!(back.book_count, 1);
        assert_eq!(back.books[0].gutenberg_id, 45109);
        assert_eq!(back.books[0].translators[0].name, "Long, George");
    }

    #[test]
    fn sha256_helper_matches_known_hash() {
        // sha256("hello world\n") = a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447
        let h = sha256_hex(b"hello world\n");
        assert_eq!(
            h,
            "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447"
        );
    }

    fn make_shard(version: u32) -> Vec<u8> {
        let shard = Shard {
            format_version: version,
            gutenberg_id: 1,
            title: "test".into(),
            translators: vec![],
            embed_model_id: "bge-small-en-v1.5".into(),
            canonical_text: "hello".into(),
            chunks: vec![],
        };
        let msgpack = rmp_serde::to_vec_named(&shard).expect("msgpack encode");
        zstd::encode_all(msgpack.as_slice(), 0).expect("zstd encode")
    }

    #[test]
    fn decode_shard_accepts_current_version() {
        let raw = make_shard(SHARD_FORMAT_VERSION);
        let decoded = decode_shard(&raw).expect("v2 shard should decode");
        assert_eq!(decoded.format_version, SHARD_FORMAT_VERSION);
        assert_eq!(decoded.gutenberg_id, 1);
    }

    #[test]
    fn decode_shard_rejects_v1() {
        // The Phase 1 bump from v1 → v2 must reject pre-bump shards loudly.
        let raw = make_shard(1);
        let err = decode_shard(&raw).expect_err("v1 shard should fail in v2 runtime");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("format_version"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn decode_shard_rejects_future_version() {
        let raw = make_shard(99);
        let err = decode_shard(&raw).expect_err("v99 shard should fail in v2 runtime");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("format_version"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn cosine_returns_one_for_identical_vectors() {
        let a = vec![0.5f32, 0.5, 0.5, 0.5];
        let sim = cosine(&a, &a);
        // After L2 normalisation cos(a, a) = 1.
        assert!((sim - 1.0).abs() < 1e-6, "got {sim}");
    }

    #[test]
    fn cosine_returns_zero_for_orthogonal_vectors() {
        let a = vec![1.0f32, 0.0, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0, 0.0];
        let sim = cosine(&a, &b);
        assert!(sim.abs() < 1e-6, "got {sim}");
    }
}
