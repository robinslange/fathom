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
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        sha256: None,
        label: "Gemma 3 4B IT (Q4_K_M)",
        size_estimate_bytes: 2_490_000_000,
    },
    ModelManifestEntry {
        id: "deberta-nli",
        filename: "deberta-v3-base-mnli-fever-anli-quantized.onnx",
        url: "https://huggingface.co/Xenova/DeBERTa-v3-base-mnli-fever-anli/resolve/main/onnx/model_quantized.onnx",
        sha256: None,
        label: "DeBERTa-v3-base MNLI/FEVER/ANLI (quantized ONNX)",
        size_estimate_bytes: 244_291_931,
    },
    ModelManifestEntry {
        id: "deberta-nli-tokenizer",
        filename: "deberta-v3-base-mnli-fever-anli-tokenizer.json",
        url: "https://huggingface.co/Xenova/DeBERTa-v3-base-mnli-fever-anli/resolve/main/tokenizer.json",
        sha256: None,
        label: "DeBERTa-v3-base NLI tokenizer",
        size_estimate_bytes: 8_656_551,
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

pub async fn ensure_model_downloaded(
    id: &str,
    progress: Option<ProgressCallback>,
) -> Result<PathBuf> {
    let entry = lookup_manifest(id).ok_or_else(|| anyhow!("unknown model id: {id}"))?;
    let dest = model_dir()?.join(entry.filename);

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

    download_streaming(entry.url, &dest, progress).await?;

    if let Some(expected) = entry.sha256 {
        let actual = sha256_file(&dest).await?;
        if !actual.eq_ignore_ascii_case(expected) {
            tokio::fs::remove_file(&dest).await.ok();
            bail!("sha256 mismatch for {id}: expected {expected}, got {actual}");
        }
    }

    Ok(dest)
}

async fn download_streaming(
    url: &str,
    dest: &Path,
    progress: Option<ProgressCallback>,
) -> Result<()> {
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

    tokio::fs::rename(&partial, dest)
        .await
        .with_context(|| format!("finalising {}", dest.display()))?;
    Ok(())
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
        assert_eq!(ids.len(), original_len, "duplicate model id in MODEL_MANIFEST");
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
        assert_eq!(
            parse_content_range_total("bytes 0-499/1234"),
            Some(1234)
        );
    }

    #[test]
    fn content_range_total_handles_unknown_size() {
        assert_eq!(parse_content_range_total("bytes 0-499/*"), None);
        assert_eq!(parse_content_range_total("bytes */12345"), Some(12345));
        assert_eq!(parse_content_range_total("garbage"), None);
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
