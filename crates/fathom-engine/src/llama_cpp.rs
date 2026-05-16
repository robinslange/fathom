//! Bundled llama.cpp backend via `llama-cpp-2`.
//!
//! Stub. Real implementation pending the first-launch model-download flow.
//! On launch the desktop app downloads a GGUF (default Gemma 3 4B IT Q4_K_M)
//! into the OS-conventional app data dir, then loads it here.

use crate::Backend;
use async_trait::async_trait;
use std::path::PathBuf;

pub struct LlamaCppBackend {
    _model_path: PathBuf,
    label: String,
}

impl LlamaCppBackend {
    pub fn new(model_path: PathBuf) -> Self {
        let label = format!(
            "{} via llama.cpp",
            model_path.file_stem().and_then(|s| s.to_str()).unwrap_or("model")
        );
        Self {
            _model_path: model_path,
            label,
        }
    }
}

#[async_trait]
impl Backend for LlamaCppBackend {
    async fn generate(&self, _prompt: &str) -> anyhow::Result<String> {
        anyhow::bail!("LlamaCppBackend not yet implemented; use OllamaBackend until model bootstrap lands")
    }

    async fn generate_json(&self, _prompt: &str) -> anyhow::Result<String> {
        anyhow::bail!("LlamaCppBackend not yet implemented")
    }

    fn model_label(&self) -> &str {
        &self.label
    }
}
