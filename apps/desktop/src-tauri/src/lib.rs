use fathom_core::library::{
    self, PassageSummary, ThemeSummary, TraditionSummary,
};
use fathom_core::{
    bootstrap, fathom_with_judge, judge, FaithfulnessScore, FathomResult, JudgeMode, Mode, Tier,
};
use fathom_engine::{LlamaCppBackend, OllamaBackend};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as AsyncMutex;

static LLAMA: OnceCell<AsyncMutex<Option<Arc<LlamaCppBackend>>>> = OnceCell::new();

fn llama_cell() -> &'static AsyncMutex<Option<Arc<LlamaCppBackend>>> {
    LLAMA.get_or_init(|| AsyncMutex::new(None))
}

#[derive(Debug, Deserialize)]
pub struct ParaphraseArgs {
    pub text: String,
    pub tier: Tier,
    pub mode: Mode,
    /// Optional override; if absent, the bundled LlamaCpp backend is used.
    pub ollama_model: Option<String>,
    pub ollama_base_url: Option<String>,
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
    // Make sure judge is ready before paraphrase so faithfulness is populated.
    ensure_judge(&app).await?;

    if let Some(model) = args.ollama_model.clone() {
        let mut backend = OllamaBackend::new(model);
        if let Some(url) = args.ollama_base_url {
            backend = backend.with_base_url(url);
        }
        Ok(fathom_with_judge(args.text, args.tier, args.mode, &backend, JudgeMode::Always(None)).await?)
    } else {
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
}

#[tauri::command]
async fn judge_pair(
    app: AppHandle,
    original: String,
    paraphrase_text: String,
) -> Result<FaithfulnessScore, AppError> {
    ensure_judge(&app).await?;
    let score = judge::score_paraphrase(original.trim(), paraphrase_text.trim())?;
    Ok(score)
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

// Silence unused warning for `Mutex` (kept for future per-task locks).
#[allow(dead_code)]
type _Pin = Mutex<()>;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            paraphrase,
            judge_pair,
            library_traditions,
            library_themes,
            library_passages,
            library_get_passage,
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
