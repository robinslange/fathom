//! Bundled llama.cpp backend via `llama-cpp-2`.
//!
//! The model file lives in the OS app data dir, downloaded on first launch by
//! `fathom_core::bootstrap`. This module is given the resolved path and loads
//! the GGUF.
//!
//! Single global `LlamaBackend` (llama.cpp requires this — initialising it twice
//! aborts). A single `LlamaContext` is allocated lazily and reused across
//! generations via `clear_kv_cache`. At `N_CTX = 32k` the KV cache is a
//! multi-hundred-MB allocation (GPU-side on Metal), so reallocating per call
//! used to be the dominant pre-first-token latency.
//!
//! `LlamaContext` wraps a `NonNull<llama_context>` and is therefore `!Send`;
//! it can't sit behind a `Mutex` shared across tokio tasks. Instead a single
//! dedicated worker thread owns the model and the context, and `generate`
//! becomes a channel round-trip: send the prompt, await the response. The
//! thread lives for the lifetime of the `LlamaCppBackend`.

use crate::Backend;
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use encoding_rs::UTF_8;
use llama_cpp_2::{
    context::params::LlamaContextParams,
    context::LlamaContext,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel},
    sampling::LlamaSampler,
};
use once_cell::sync::OnceCell;
use std::num::NonZeroU32;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

// Gemma 3 4B's native context is ~131k tokens. We use 32k to give plenty of
// headroom for the global substrate map (~20k tokens once Slice 1's harvest
// landed) plus passage + output, without paying the full KV-cache cost of
// the native limit. v0.21 plans to swap "inject all terms" for a semantic-
// ranked subset that fits in ~4k, at which point this can drop back down.
const N_CTX: u32 = 32768;
const MAX_NEW_TOKENS: u32 = 2000;
const TEMPERATURE: f32 = 0.2;
const SEED: u32 = 1234;

static LLAMA_BACKEND: OnceCell<LlamaBackend> = OnceCell::new();

fn get_backend() -> Result<&'static LlamaBackend> {
    LLAMA_BACKEND.get_or_try_init(|| {
        let mut backend =
            LlamaBackend::init().map_err(|e| anyhow!("llama backend init failed: {e}"))?;
        backend.void_logs();
        unsafe {
            llama_cpp_sys_2::ggml_log_set(Some(void_log), std::ptr::null_mut());
        }
        Ok::<LlamaBackend, anyhow::Error>(backend)
    })
}

unsafe extern "C" fn void_log(
    _level: llama_cpp_sys_2::ggml_log_level,
    _text: *const std::os::raw::c_char,
    _user_data: *mut std::os::raw::c_void,
) {
}

/// One unit of work for the worker thread.
struct Job {
    prompt: String,
    reply: oneshot::Sender<Result<String>>,
}

pub struct LlamaCppBackend {
    tx: mpsc::Sender<Job>,
    label: String,
}

impl LlamaCppBackend {
    /// Load a GGUF model. Path must point at an existing file; use
    /// `fathom_core::bootstrap::ensure_model_downloaded` to fetch on first launch.
    ///
    /// Spawns the worker thread that owns the `LlamaModel` and `LlamaContext`.
    /// The context is built lazily on the first job so model-load failures
    /// surface here while context allocation (which can take seconds at 32k)
    /// doesn't block the caller.
    pub fn load(model_path: PathBuf) -> Result<Self> {
        let backend = get_backend()?;
        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(backend, &model_path, &model_params)
            .with_context(|| format!("failed to load GGUF at {}", model_path.display()))?;
        let label = format!(
            "{} via llama.cpp",
            model_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("model")
        );

        // Bounded channel so an overactive caller can't queue more work than
        // the worker can drain. 4 is a guess; bump if we ever need more.
        let (tx, rx) = mpsc::channel::<Job>(4);

        // Hand the model + receiver off to a dedicated OS thread. We use
        // std::thread rather than tokio::task::spawn_blocking because we need
        // a stable thread identity for the lifetime of the backend — the
        // LlamaContext lives on this thread and never moves.
        //
        // The rx is a tokio mpsc receiver, which can only be polled on a tokio
        // runtime. We bridge by using blocking_recv on the worker thread.
        std::thread::Builder::new()
            .name("fathom-llama-worker".into())
            .spawn(move || worker_loop(model, rx))
            .context("spawning llama worker thread")?;

        Ok(Self { tx, label })
    }
}

fn worker_loop(model: LlamaModel, mut rx: mpsc::Receiver<Job>) {
    let mut ctx: Option<LlamaContext<'_>> = None;
    while let Some(job) = rx.blocking_recv() {
        let result = decode_with_ctx(&model, &mut ctx, &job.prompt);
        // Caller may have dropped the oneshot if they were cancelled; ignore.
        let _ = job.reply.send(result);
    }
}

/// Decode a single prompt, allocating the context lazily on first call and
/// reusing it (with `clear_kv_cache`) on subsequent calls.
fn decode_with_ctx<'m>(
    model: &'m LlamaModel,
    ctx_slot: &mut Option<LlamaContext<'m>>,
    prompt: &str,
) -> Result<String> {
    let backend = get_backend()?;

    // n_batch must accommodate the entire prompt in a single decode call —
    // the default of 512 triggers a GGML_ASSERT abort (visible as a SIGABRT
    // on a tokio worker thread) when we feed it the ~20k-token prompt that
    // global-substrate injection produces. Keep n_ubatch smaller to control
    // peak compute memory; llama-cpp will split internally.
    if ctx_slot.is_none() {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_batch(N_CTX)
            .with_n_ubatch(512);
        let ctx = model
            .new_context(backend, ctx_params)
            .context("creating llama context")?;
        *ctx_slot = Some(ctx);
    }
    let ctx = ctx_slot.as_mut().expect("ctx allocated above");
    ctx.clear_kv_cache();

    let templated = format!(
        "<start_of_turn>user\n{prompt}<end_of_turn>\n<start_of_turn>model\n"
    );

    let tokens_in = model
        .str_to_token(&templated, AddBos::Always)
        .context("tokenization failed")?;
    let n_in = tokens_in.len() as i32;
    if (n_in as u32) >= N_CTX {
        bail!("prompt too long: {} tokens >= n_ctx {}", n_in, N_CTX);
    }

    // Batch must hold the full prompt; the prior 512 limit panicked with
    // "Insufficient Space" once the global substrate map pushed prompts past
    // a few hundred tokens. Match N_CTX so any prompt that fits the context
    // also fits the batch.
    let mut batch = LlamaBatch::new(N_CTX as usize, 1);
    for (i, token) in tokens_in.iter().enumerate() {
        let is_last = i == tokens_in.len() - 1;
        batch.add(*token, i as i32, &[0], is_last)?;
    }
    ctx.decode(&mut batch).context("initial decode failed")?;

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::temp(TEMPERATURE),
        LlamaSampler::dist(SEED),
    ]);

    let mut output = String::new();
    let mut decoder = UTF_8.new_decoder();
    let mut n_decoded: u32 = 0;
    let mut pos = n_in;

    while n_decoded < MAX_NEW_TOKENS {
        let token = sampler.sample(ctx, batch.n_tokens() - 1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, false, None)
            .context("token_to_piece failed")?;
        output.push_str(&piece);

        batch.clear();
        batch.add(token, pos, &[0], true)?;
        pos += 1;
        n_decoded += 1;
        ctx.decode(&mut batch).context("decode step failed")?;
    }

    Ok(output)
}

#[async_trait]
impl Backend for LlamaCppBackend {
    async fn generate(&self, prompt: &str) -> Result<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let job = Job {
            prompt: prompt.to_string(),
            reply: reply_tx,
        };
        self.tx
            .send(job)
            .await
            .map_err(|_| anyhow!("llama worker thread has shut down"))?;
        reply_rx
            .await
            .map_err(|_| anyhow!("llama worker dropped reply channel"))?
    }

    async fn generate_json(&self, prompt: &str) -> Result<String> {
        // No GBNF grammar yet; the JIT identify pass tolerates loose JSON via parser fallbacks.
        self.generate(prompt).await
    }

    fn model_label(&self) -> &str {
        &self.label
    }
}
