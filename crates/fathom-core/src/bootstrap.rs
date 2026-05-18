//! Model bootstrap.
//!
//! Fathom keeps its binary small. Heavy model files (the LLM GGUF, the NLI ONNX)
//! are downloaded on first launch into the OS-conventional app data directory
//! rather than bundled. This module owns the manifest of expected models, the
//! resolution to a concrete path, and the streaming download + verification.
//!
//! Manifest entries are baked into the binary; updates ship via release.

use anyhow::{anyhow, bail, Context, Result};
use directories::ProjectDirs;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Debug, Clone, Copy)]
pub struct ModelManifestEntry {
    pub id: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    /// SHA-256 hex digest. `None` skips verification (dev only).
    pub sha256: Option<&'static str>,
    pub label: &'static str,
    pub size_estimate_bytes: u64,
}

/// All models Fathom knows how to download. Stable IDs; URLs may change between releases.
pub const MODEL_MANIFEST: &[ModelManifestEntry] = &[
    ModelManifestEntry {
        id: "gemma3-4b",
        filename: "gemma-3-4b-it-Q4_K_M.gguf",
        url: "https://huggingface.co/bartowski/google_gemma-3-4b-it-GGUF/resolve/main/google_gemma-3-4b-it-Q4_K_M.gguf",
        sha256: Some("4996030242583a40aa151ff93f49ed787ac8c25e4120c3ae4588b2e2a7d1ae94"),
        label: "Gemma 3 4B IT (Q4_K_M)",
        size_estimate_bytes: 2_490_000_000,
    },
    ModelManifestEntry {
        id: "deberta-nli",
        filename: "deberta-v3-base-mnli-fever-anli-quantized.onnx",
        url: "https://huggingface.co/Xenova/DeBERTa-v3-base-mnli-fever-anli/resolve/main/onnx/model_quantized.onnx",
        sha256: Some("ab9da76bb06054ea6b921560c1ecf5683a9e4d96f0ea73d78d3b4a8990aea882"),
        label: "DeBERTa-v3-base MNLI/FEVER/ANLI (quantized ONNX)",
        size_estimate_bytes: 244_291_931,
    },
    ModelManifestEntry {
        id: "deberta-nli-tokenizer",
        filename: "deberta-v3-base-mnli-fever-anli-tokenizer.json",
        url: "https://huggingface.co/Xenova/DeBERTa-v3-base-mnli-fever-anli/resolve/main/tokenizer.json",
        sha256: Some("a86f883318afa11c8c10466f1bf4efaeb6ded28a52cbe57217a8fa0d0a2a87df"),
        label: "DeBERTa-v3-base NLI tokenizer",
        size_estimate_bytes: 8_656_551,
    },
    ModelManifestEntry {
        id: "bge-small",
        filename: "bge-small-en-v1.5.onnx",
        url: "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/onnx/model.onnx",
        sha256: Some("828e1496d7fabb79cfa4dcd84fa38625c0d3d21da474a00f08db0f559940cf35"),
        label: "BAAI bge-small-en-v1.5 (384-dim sentence embeddings)",
        size_estimate_bytes: 133_000_000,
    },
    ModelManifestEntry {
        id: "bge-small-tokenizer",
        filename: "bge-small-en-v1.5-tokenizer.json",
        url: "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/tokenizer.json",
        sha256: Some("d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66"),
        label: "bge-small tokenizer",
        size_estimate_bytes: 711_396,
    },
];

pub fn lookup_manifest(id: &str) -> Option<ModelManifestEntry> {
    MODEL_MANIFEST.iter().copied().find(|m| m.id == id)
}

pub fn model_dir() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("nz", "omit", "fathom")
        .ok_or_else(|| anyhow!("could not resolve OS project directories"))?;
    let dir = proj_dirs.data_dir().join("models");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating model dir {}", dir.display()))?;
    Ok(dir)
}

pub fn model_path(id: &str) -> Result<PathBuf> {
    let entry = lookup_manifest(id).ok_or_else(|| anyhow!("unknown model id: {id}"))?;
    Ok(model_dir()?.join(entry.filename))
}

pub fn is_downloaded(id: &str) -> Result<bool> {
    let path = model_path(id)?;
    Ok(path.is_file())
}

/// `(bytes_downloaded, total_bytes_if_known)`.
pub type ProgressCallback = Box<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Per-model-id locks. Concurrent callers for the same model serialise on the
/// inner `AsyncMutex`; the first finishes the download and the rest fast-path
/// out via the on-disk SHA check. Without this, two concurrent invocations
/// race on the shared `.partial` file — both append the response body, and
/// the partial grows to ~2-4× the expected size with interleaved garbage,
/// guaranteeing a SHA mismatch and an `ENOENT` from one task whose partial
/// got deleted out from under it by the other's failure path.
///
/// The outer `StdMutex` is held only for the get-or-insert HashMap op; it is
/// never held across an `.await`. The inner `AsyncMutex` is what callers
/// actually contend on.
type DownloadLockMap = HashMap<&'static str, Arc<AsyncMutex<()>>>;

static DOWNLOAD_LOCKS: Lazy<StdMutex<DownloadLockMap>> =
    Lazy::new(|| StdMutex::new(HashMap::new()));

fn lock_for(id: &'static str) -> Arc<AsyncMutex<()>> {
    let mut map = DOWNLOAD_LOCKS.lock().expect("download lock map poisoned");
    map.entry(id)
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

pub async fn ensure_model_downloaded(
    id: &str,
    progress: Option<ProgressCallback>,
) -> Result<PathBuf> {
    let entry = lookup_manifest(id).ok_or_else(|| anyhow!("unknown model id: {id}"))?;
    let dest = model_dir()?.join(entry.filename);

    let lock = lock_for(entry.id);
    let _guard = lock.lock().await;

    if dest.exists() {
        match entry.sha256 {
            Some(expected) => {
                let actual = sha256_file(&dest).await?;
                if actual.eq_ignore_ascii_case(expected) {
                    return Ok(dest);
                }
                tokio::fs::remove_file(&dest).await.ok();
            }
            None => return Ok(dest),
        }
    }

    let partial = download_streaming(entry.url, &dest, progress).await?;

    if let Some(expected) = entry.sha256 {
        // Verify the .partial before promoting it to dest — a corrupted
        // download must never land at the canonical path, or future
        // is_downloaded() checks will return a poisoned cache hit.
        let actual = sha256_file(&partial).await?;
        if !actual.eq_ignore_ascii_case(expected) {
            tokio::fs::remove_file(&partial).await.ok();
            bail!("sha256 mismatch for {id}: expected {expected}, got {actual}");
        }
    }

    tokio::fs::rename(&partial, &dest)
        .await
        .with_context(|| format!("finalising {}", dest.display()))?;
    Ok(dest)
}

/// Download to a `.partial` sibling of `dest`. Returns the partial path so the
/// caller can verify it before renaming to the canonical destination.
async fn download_streaming(
    url: &str,
    dest: &Path,
    progress: Option<ProgressCallback>,
) -> Result<PathBuf> {
    let partial = partial_path(dest);
    let client = reqwest::Client::builder()
        .user_agent(concat!("fathom/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let existing = match tokio::fs::metadata(&partial).await {
        Ok(meta) if meta.is_file() => meta.len(),
        _ => 0,
    };

    let mut request = client.get(url);
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?
        .error_for_status()
        .with_context(|| format!("download failed: {url}"))?;

    let status = response.status();
    let (mut bytes_so_far, total, append) = match status {
        StatusCode::PARTIAL_CONTENT => {
            let full = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_content_range_total);
            (existing, full, true)
        }
        _ => (0, response.content_length(), false),
    };

    let mut file = if append {
        OpenOptions::new()
            .append(true)
            .open(&partial)
            .await
            .with_context(|| format!("opening partial for append {}", partial.display()))?
    } else {
        File::create(&partial)
            .await
            .with_context(|| format!("creating partial file {}", partial.display()))?
    };

    if let Some(cb) = &progress {
        cb(bytes_so_far, total);
    }

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        bytes_so_far += chunk.len() as u64;
        if let Some(cb) = &progress {
            cb(bytes_so_far, total);
        }
    }
    file.flush().await?;
    drop(file);

    Ok(partial)
}

/// Parse the `/Z` part of a `Content-Range: bytes X-Y/Z` (or `bytes */Z`) header
/// into the total file size. Returns `None` if the size is `*` (unknown) or the
/// header is malformed.
fn parse_content_range_total(value: &str) -> Option<u64> {
    let after_slash = value.rsplit('/').next()?.trim();
    if after_slash == "*" {
        None
    } else {
        after_slash.parse::<u64>().ok()
    }
}

fn partial_path(dest: &Path) -> PathBuf {
    let ext = dest
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!("{e}.partial"))
        .unwrap_or_else(|| "partial".to_string());
    dest.with_extension(ext)
}

async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_ids_are_unique() {
        let mut ids: Vec<&str> = MODEL_MANIFEST.iter().map(|m| m.id).collect();
        ids.sort();
        let original_len = ids.len();
        ids.dedup();
        assert_eq!(
            ids.len(),
            original_len,
            "duplicate model id in MODEL_MANIFEST"
        );
    }

    #[test]
    fn lookup_known_and_unknown() {
        assert!(lookup_manifest("gemma3-4b").is_some());
        assert!(lookup_manifest("deberta-nli").is_some());
        assert!(lookup_manifest("not-a-real-id").is_none());
    }

    #[test]
    fn partial_path_appends_partial() {
        let p = partial_path(Path::new("/tmp/foo.gguf"));
        assert_eq!(p, PathBuf::from("/tmp/foo.gguf.partial"));
        let q = partial_path(Path::new("/tmp/noext"));
        assert_eq!(q, PathBuf::from("/tmp/noext.partial"));
    }

    #[test]
    fn content_range_total_parses_typical_header() {
        assert_eq!(
            parse_content_range_total("bytes 1000-2489999999/2490000000"),
            Some(2_490_000_000)
        );
        assert_eq!(parse_content_range_total("bytes 0-499/1234"), Some(1234));
    }

    #[test]
    fn content_range_total_handles_unknown_size() {
        assert_eq!(parse_content_range_total("bytes 0-499/*"), None);
        assert_eq!(parse_content_range_total("bytes */12345"), Some(12345));
        assert_eq!(parse_content_range_total("garbage"), None);
    }

    #[test]
    fn lock_for_returns_same_arc_per_id() {
        let a1 = lock_for("gemma3-4b");
        let a2 = lock_for("gemma3-4b");
        let b1 = lock_for("deberta-nli");
        assert!(
            Arc::ptr_eq(&a1, &a2),
            "lock_for must return the same Arc for the same id so concurrent callers coalesce"
        );
        assert!(
            !Arc::ptr_eq(&a1, &b1),
            "lock_for must return distinct Arcs for distinct ids so independent models don't serialise"
        );
    }

    #[tokio::test]
    async fn concurrent_ensure_for_same_id_serialises() -> Result<()> {
        use tokio::net::TcpListener;

        // 256 bytes deterministic content + its known SHA-256
        let payload: Vec<u8> = (0..=255u8).collect();
        let payload_sha = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(&payload);
            format!("{:x}", h.finalize())
        };

        // Minimal HTTP/1.1 server. Counts hits so we can assert the lock
        // coalesced 8 callers into a single network fetch.
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hits_srv = hits.clone();
        let payload_srv = payload.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => return,
                };
                hits_srv.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let payload = payload_srv.clone();
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt as _;
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                        payload.len()
                    );
                    let mut resp = header.into_bytes();
                    resp.extend_from_slice(&payload);
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut sock, &resp).await;
                    let _ = tokio::io::AsyncWriteExt::shutdown(&mut sock).await;
                });
            }
        });

        // Sanity: tighter test that doesn't depend on the real manifest — we
        // exercise lock_for + download_streaming through a private helper that
        // mirrors ensure_model_downloaded's body but takes an explicit url +
        // dest + lock-id. This keeps the test hermetic.
        let dir = tempfile_dir()?.join(format!("concurrent-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await?;
        let dest = dir.join("payload.bin");
        let url = format!("http://{addr}/payload.bin");

        async fn one(
            id: &'static str,
            url: String,
            dest: PathBuf,
            expected: String,
        ) -> Result<PathBuf> {
            let lock = lock_for(id);
            let _guard = lock.lock().await;
            if dest.exists() {
                let actual = sha256_file(&dest).await?;
                if actual.eq_ignore_ascii_case(&expected) {
                    return Ok(dest);
                }
                tokio::fs::remove_file(&dest).await.ok();
            }
            let partial = download_streaming(&url, &dest, None).await?;
            let actual = sha256_file(&partial).await?;
            if !actual.eq_ignore_ascii_case(&expected) {
                tokio::fs::remove_file(&partial).await.ok();
                bail!("sha mismatch: expected {expected} got {actual}");
            }
            tokio::fs::rename(&partial, &dest).await?;
            Ok(dest)
        }

        let mut joins = Vec::new();
        for _ in 0..8 {
            let u = url.clone();
            let d = dest.clone();
            let s = payload_sha.clone();
            joins.push(tokio::spawn(async move {
                one("__concurrent-test-id", u, d, s).await
            }));
        }
        for j in joins {
            let path = j.await??;
            // Every caller observes the same final file.
            let bytes = tokio::fs::read(&path).await?;
            assert_eq!(bytes, payload, "final file must equal canonical payload");
        }

        // The whole point: 8 concurrent callers must produce exactly 1 fetch.
        let n = hits.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            n, 1,
            "expected 1 HTTP request (callers coalesce via per-id lock), got {n}"
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
        Ok(())
    }

    #[tokio::test]
    async fn sha256_of_empty_file_is_known() -> Result<()> {
        let dir = tempfile_dir()?;
        let path = dir.join("empty");
        tokio::fs::write(&path, b"").await?;
        let h = sha256_file(&path).await?;
        assert_eq!(
            h,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        tokio::fs::remove_file(&path).await?;
        Ok(())
    }

    fn tempfile_dir() -> Result<PathBuf> {
        let dir = std::env::temp_dir().join("fathom-bootstrap-tests");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}
