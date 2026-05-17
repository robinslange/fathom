use fathom_core::runtime::{ManifestBook, Runtime, SearchHit, Shard};
use fathom_core::{
    bootstrap, fathom_with_judge, judge, FathomResult, JudgeMode,
    Mode, Tier,
};
use fathom_embed;
use fathom_engine::LlamaCppBackend;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as AsyncMutex;

static LLAMA: OnceCell<AsyncMutex<Option<Arc<LlamaCppBackend>>>> = OnceCell::new();

fn llama_cell() -> &'static AsyncMutex<Option<Arc<LlamaCppBackend>>> {
    LLAMA.get_or_init(|| AsyncMutex::new(None))
}

static RUNTIME: OnceCell<AsyncMutex<Option<Arc<Runtime>>>> = OnceCell::new();

fn runtime_cell() -> &'static AsyncMutex<Option<Arc<Runtime>>> {
    RUNTIME.get_or_init(|| AsyncMutex::new(None))
}

async fn ensure_runtime() -> Result<Arc<Runtime>, AppError> {
    let mut guard = runtime_cell().lock().await;
    if let Some(r) = guard.as_ref() {
        return Ok(r.clone());
    }
    let manifest = fathom_core::runtime::fetch_manifest().await?;
    let r = Arc::new(Runtime::new(manifest));
    *guard = Some(r.clone());
    Ok(r)
}

#[derive(Debug, Deserialize)]
pub struct ParaphraseArgs {
    pub text: String,
    pub tier: Tier,
    pub mode: Mode,
}

#[derive(Debug, Serialize, Clone)]
pub struct AppError {
    pub message: String,
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self {
            message: format!("{e:#}"),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
struct DownloadProgress {
    /// Manifest id of the model being fetched.
    model: String,
    /// Bytes downloaded so far.
    bytes: u64,
    /// Total bytes if reported by server.
    total: Option<u64>,
}

fn make_progress_callback(handle: AppHandle, model_id: &'static str) -> bootstrap::ProgressCallback {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    struct Throttle {
        last_at: Instant,
    }
    let state = Mutex::new(Throttle {
        last_at: Instant::now() - Duration::from_secs(60),
    });

    Box::new(move |bytes, total| {
        let is_final = total.is_some_and(|t| bytes >= t);
        let mut s = state.lock().expect("progress mutex poisoned");
        if !is_final && s.last_at.elapsed() < Duration::from_millis(200) {
            return;
        }
        s.last_at = Instant::now();
        let _ = handle.emit(
            "fathom://download-progress",
            DownloadProgress {
                model: model_id.to_string(),
                bytes,
                total,
            },
        );
    })
}

async fn ensure_llama(handle: &AppHandle) -> Result<Arc<LlamaCppBackend>, AppError> {
    let mut guard = llama_cell().lock().await;
    if let Some(b) = guard.as_ref() {
        return Ok(b.clone());
    }
    let path: PathBuf = bootstrap::ensure_model_downloaded(
        "gemma3-4b",
        Some(make_progress_callback(handle.clone(), "gemma3-4b")),
    )
    .await?;
    let backend = LlamaCppBackend::load(path)?;
    let arc = Arc::new(backend);
    *guard = Some(arc.clone());
    Ok(arc)
}

async fn ensure_judge(handle: &AppHandle) -> Result<(), AppError> {
    judge::ensure_loaded(Some(make_progress_callback(handle.clone(), "deberta-nli"))).await?;
    Ok(())
}

#[tauri::command]
async fn paraphrase(app: AppHandle, args: ParaphraseArgs) -> Result<FathomResult, AppError> {
    ensure_judge(&app).await?;
    let llama = ensure_llama(&app).await?;
    Ok(fathom_with_judge(
        args.text,
        args.tier,
        args.mode,
        llama.as_ref(),
        JudgeMode::Always(None),
    )
    .await?)
}

#[derive(Serialize)]
pub struct BookView {
    pub gutenberg_id: u32,
    pub title: String,
    pub translators: Vec<String>,
    pub canonical_text: String,
    pub chunks: Vec<ChunkRefView>,
}

#[derive(Serialize)]
pub struct ChunkRefView {
    pub chunk_id: String,
    pub char_offset_start: usize,
    pub char_offset_end: usize,
}

#[tauri::command]
async fn library_manifest() -> Result<Vec<ManifestBook>, AppError> {
    let rt = ensure_runtime().await?;
    Ok(rt.manifest().books.clone())
}

#[tauri::command]
async fn library_search(query: String, top_n: usize) -> Result<Vec<SearchHit>, AppError> {
    let rt = ensure_runtime().await?;
    Ok(rt.search(&query, top_n).await?)
}

#[tauri::command]
async fn library_load_book(gutenberg_id: u32) -> Result<BookView, AppError> {
    let rt = ensure_runtime().await?;
    let shard: Arc<Shard> = rt.ensure_shard(gutenberg_id).await?;
    let translators = shard
        .translators
        .iter()
        .map(|t| t.name.clone())
        .collect();
    let chunks = shard
        .chunks
        .iter()
        .map(|c| ChunkRefView {
            chunk_id: c.chunk_id.clone(),
            char_offset_start: c.char_offset_start,
            char_offset_end: c.char_offset_end,
        })
        .collect();
    Ok(BookView {
        gutenberg_id: shard.gutenberg_id,
        title: shard.title.clone(),
        translators,
        canonical_text: shard.canonical_text.clone(),
        chunks,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryParaphraseArgs {
    pub gutenberg_id: u32,
    pub start_byte: usize,
    pub end_byte: usize,
    pub tier: Tier,
}

#[tauri::command]
async fn library_paraphrase_selection(
    app: AppHandle,
    args: LibraryParaphraseArgs,
) -> Result<FathomResult, AppError> {
    let rt = ensure_runtime().await?;
    let snapped = rt
        .snap_selection(args.gutenberg_id, args.start_byte, args.end_byte)
        .await?
        .ok_or_else(|| AppError {
            message: "selection outside any sentence".to_string(),
        })?;
    let shard = rt.ensure_shard(args.gutenberg_id).await?;
    let text = shard.canonical_text[snapped.0..snapped.1].to_string();

    ensure_judge(&app).await?;
    let llama = ensure_llama(&app).await?;
    // Mode::Auto: try the curated 135-passage seed lexicon by fingerprint
    // first (rare hit on arbitrary Gutenberg prose), fall through to JIT
    // identification + gloss-with-guard (Gemma identifies terms-of-art in
    // the selection and glosses them on the fly), then no-substrate as a
    // last resort. Curated-only would error for almost every selection.
    // v0.21 plans semantic substrate retrieval — rank the global substrate
    // map against the selection's embedding and inject the top-N relevant
    // terms — but that's a separate piece of work.
    Ok(fathom_with_judge(
        text,
        args.tier,
        fathom_core::Mode::Auto,
        llama.as_ref(),
        JudgeMode::Always(None),
    )
    .await?)
}

#[tauri::command]
async fn library_ensure_embedder(app: AppHandle) -> Result<(), AppError> {
    let model = bootstrap::ensure_model_downloaded(
        "bge-small",
        Some(make_progress_callback(app.clone(), "bge-small")),
    )
    .await?;
    let tokenizer = bootstrap::ensure_model_downloaded(
        "bge-small-tokenizer",
        Some(make_progress_callback(app, "bge-small-tokenizer")),
    )
    .await?;
    fathom_embed::init_embedder(&model, &tokenizer)
        .map_err(|e| AppError { message: format!("init embedder: {e:#}") })?;
    Ok(())
}

/// Eagerly fetch + decode the first N shards (alphabetical by author then
/// title) so library_search has something to rank on first query. Returns
/// the number of shards now resident in the LRU. Idempotent.
///
/// Fetches in parallel with bounded concurrency (8 in flight) to keep the
/// first-launch wall-clock under ~5s on typical home broadband for the
/// default limit=64. Sequential awaits would be ~13s due to per-request
/// round-trip overhead.
#[tauri::command]
async fn library_prewarm_shards(limit: usize) -> Result<usize, AppError> {
    use futures_util::stream::StreamExt;
    let rt = ensure_runtime().await?;
    let mut books: Vec<ManifestBook> = rt.manifest().books.clone();
    books.sort_by(|a, b| {
        let a_auth = a.translators.first().map(|t| t.name.as_str()).unwrap_or("");
        let b_auth = b.translators.first().map(|t| t.name.as_str()).unwrap_or("");
        a_auth.cmp(b_auth).then_with(|| a.title.cmp(&b.title))
    });
    let mut stream = futures_util::stream::iter(
        books.into_iter().take(limit).map(|book| {
            let rt = rt.clone();
            async move {
                match rt.ensure_shard(book.gutenberg_id).await {
                    Ok(_) => 1usize,
                    Err(e) => {
                        eprintln!("prewarm shard {} failed: {e:#}", book.gutenberg_id);
                        0
                    }
                }
            }
        }),
    )
    .buffer_unordered(8);
    let mut warmed = 0usize;
    while let Some(n) = stream.next().await {
        warmed += n;
    }
    Ok(warmed)
}

#[tauri::command]
async fn library_favourite(gutenberg_id: u32, on: bool) -> Result<(), AppError> {
    let path = favourites_path()?;
    let mut current = read_favourites_inner(&path).await.unwrap_or_default();
    if on {
        if !current.contains(&gutenberg_id) {
            current.push(gutenberg_id);
        }
    } else {
        current.retain(|id| *id != gutenberg_id);
    }
    current.sort();
    write_favourites_inner(&path, &current).await
}

#[tauri::command]
async fn library_favourites() -> Result<Vec<u32>, AppError> {
    let path = favourites_path()?;
    Ok(read_favourites_inner(&path).await.unwrap_or_default())
}

fn favourites_path() -> Result<PathBuf, AppError> {
    let proj = directories::ProjectDirs::from("nz", "omit", "fathom")
        .ok_or_else(|| AppError {
            message: "project dirs unavailable".to_string(),
        })?;
    let dir = proj.data_dir().join("state");
    std::fs::create_dir_all(&dir).map_err(|e| AppError {
        message: format!("create state dir: {e}"),
    })?;
    Ok(dir.join("favourites.json"))
}

async fn read_favourites_inner(path: &Path) -> Result<Vec<u32>, AppError> {
    let bytes = tokio::fs::read(path).await.map_err(|e| AppError {
        message: format!("read favourites: {e}"),
    })?;
    let v: Vec<u32> = serde_json::from_slice(&bytes).map_err(|e| AppError {
        message: format!("parse favourites: {e}"),
    })?;
    Ok(v)
}

async fn write_favourites_inner(path: &Path, v: &[u32]) -> Result<(), AppError> {
    let bytes = serde_json::to_vec_pretty(v).map_err(|e| AppError {
        message: format!("encode favourites: {e}"),
    })?;
    tokio::fs::write(path, &bytes).await.map_err(|e| AppError {
        message: format!("write favourites: {e}"),
    })?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            paraphrase,
            library_manifest,
            library_search,
            library_load_book,
            library_paraphrase_selection,
            library_ensure_embedder,
            library_prewarm_shards,
            library_favourite,
            library_favourites,
        ])
        .build(tauri::generate_context!())
        .expect("error while building fathom desktop");

    // Bypass C++ static destructors on shutdown. llama.cpp's `LlamaBackend`
    // and ort's onnxruntime both register global teardown that SIGABRTs at
    // process exit on macOS — visible to the user as a "Fathom closed
    // unexpectedly" dialog after Cmd+Q. We call libc::_exit (underscore
    // variant: skips atexit handlers and C++ static destructors entirely)
    // before AppKit's normal shutdown gets a chance to abort us. By the
    // time ExitRequested fires, the user has already asked to quit and any
    // pending writes from our own code have already flushed.
    app.run(|_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = &event {
            unsafe {
                libc::_exit(0);
            }
        }
    });
}
