//! Backend abstraction.
//!
//! Three backends planned:
//!
//! - [`OllamaBackend`] — local Ollama HTTP server (the connector option).
//!   Implemented now; useful for dev + power users who already run Ollama.
//! - [`LlamaCppBackend`] — bundled inference via `llama-cpp-2`, the default
//!   for the desktop app. Stub for now; implementation lands when the
//!   first-launch model-download flow is built.
//! - [`HttpBackend`] — Anthropic/OpenAI completions-compat endpoints, for
//!   users who want cloud inference instead of local. Stub for now.

use async_trait::async_trait;

pub mod ollama;
pub mod llama_cpp;
pub mod http;

pub use http::HttpBackend;
pub use llama_cpp::LlamaCppBackend;
pub use ollama::OllamaBackend;

#[async_trait]
pub trait Backend: Send + Sync {
    /// Plain prose generation. Used by all glossing paths.
    async fn generate(&self, prompt: &str) -> anyhow::Result<String>;

    /// JSON-formatted generation. Used by the JIT identify pass.
    /// May fall back to prose if the backend doesn't support JSON mode.
    async fn generate_json(&self, prompt: &str) -> anyhow::Result<String>;

    /// Human-readable label (e.g. "gemma3:4b via ollama") — used for
    /// the `model` field in `FathomResult`.
    fn model_label(&self) -> &str;
}
