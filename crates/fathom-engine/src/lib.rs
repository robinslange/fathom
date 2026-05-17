//! Backend abstraction.
//!
//! Two backends:
//!
//! - [`OllamaBackend`] — local Ollama HTTP server. Useful for dev + power
//!   users who already run Ollama.
//! - [`LlamaCppBackend`] — bundled inference via `llama-cpp-2`, the default
//!   for the desktop app.

use async_trait::async_trait;

pub mod llama_cpp;
pub mod ollama;

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
