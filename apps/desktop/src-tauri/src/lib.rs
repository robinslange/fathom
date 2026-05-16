use fathom_core::library::{
    self, PassageSummary, ThemeSummary, TraditionSummary,
};
use fathom_core::runtime::{ManifestBook, Runtime, SearchHit, Shard};
use fathom_core::{
    bootstrap, fathom_with_global_substrate, fathom_with_judge, judge, FathomResult, JudgeMode,
    Mode, Tier,
};
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

#[tauri::command]
fn library_traditions() -> Vec<TraditionSummary> {
    library::list_traditions()
}

#[tauri::command]
fn library_themes() -> Vec<ThemeSummary> {
    library::list_themes()
}

#[tauri::command]
fn library_passages(
    tradition: Option<String>,
    theme: Option<String>,
) -> Result<Vec<PassageSummary>, AppError> {
    Ok(match (tradition, theme) {
        (Some(_), Some(_)) => return Err(anyhow::anyhow!(
            "pass exactly one of `tradition` or `theme`, not both"
        )
        .into()),
        (Some(t), None) => library::list_passages_by_tradition(&t),
        (None, Some(t)) => library::list_passages_by_theme(&t),
        (None, None) => library::list_all_passages(),
    })
}

#[derive(Serialize)]
pub struct PassageDetail {
    pub id: String,
    pub fingerprint: String,
    pub author: String,
    pub title: String,
    pub translation: String,
    pub language: String,
    pub tradition: String,
    pub themes: Vec<String>,
    pub terms: Vec<TermView>,
}

#[derive(Serialize)]
pub struct TermView {
    pub term: String,
    pub substrate: String,
    pub gloss: String,
}

#[tauri::command]
fn library_get_passage(id: String) -> Option<PassageDetail> {
    library::get_passage(&id).map(|e| PassageDetail {
        id: e.passage.id.clone(),
        fingerprint: e.passage.fingerprint.clone(),
        author: e.source.author.clone(),
        title: e.source.title.clone(),
        translation: e.source.translation.clone(),
        language: e.source.language.clone(),
        tradition: e.source.tradition.clone(),
        themes: e.passage.themes.clone(),
        terms: e
            .passage
            .terms
            .iter()
            .map(|(term, info)| TermView {
                term: term.clone(),
                substrate: info.substrate.clone(),
                gloss: info.gloss.clone(),
            })
            .collect(),
    })
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
pub struct LibraryParaphraseArgs {
    pub gutenberg_id: u32,
    pub chunk_id: String,
    pub sel_start: usize,
    pub sel_end: usize,
    pub tier: Tier,
}

// Build the global substrate map once at first call; reuse across paraphrases.
static GLOBAL_SUBSTRATE: OnceCell<std::collections::BTreeMap<String, fathom_core::lexicon::TermEntry>> =
    OnceCell::new();

fn global_substrate(
) -> &'static std::collections::BTreeMap<String, fathom_core::lexicon::TermEntry> {
    GLOBAL_SUBSTRATE.get_or_init(fathom_core::lexicon::global_substrate_map)
}

#[tauri::command]
async fn library_paraphrase_selection(
    app: AppHandle,
    args: LibraryParaphraseArgs,
) -> Result<FathomResult, AppError> {
    let rt = ensure_runtime().await?;
    let snapped = rt
        .snap_selection(args.gutenberg_id, &args.chunk_id, args.sel_start, args.sel_end)
        .await?
        .ok_or_else(|| AppError {
            message: "selection outside any sentence".to_string(),
        })?;
    let shard = rt.ensure_shard(args.gutenberg_id).await?;
    let text = shard.canonical_text[snapped.0..snapped.1].to_string();

    ensure_judge(&app).await?;
    let llama = ensure_llama(&app).await?;
    Ok(fathom_with_global_substrate(
        text,
        args.tier,
        llama.as_ref(),
        global_substrate(),
        JudgeMode::Always(None),
    )
    .await?)
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
            library_traditions,
            library_themes,
            library_passages,
            library_get_passage,
            library_manifest,
            library_search,
            library_load_book,
            library_paraphrase_selection,
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
