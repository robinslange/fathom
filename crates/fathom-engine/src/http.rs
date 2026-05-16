//! HTTP backend for cloud completions endpoints.
//!
//! Stub. Will support Anthropic Messages API and OpenAI Chat Completions
//! (or any compatible endpoint). For users who want cloud inference rather
//! than local — useful when the user's hardware can't fit a 4B model.

use crate::Backend;
use async_trait::async_trait;

pub struct HttpBackend {
    _endpoint: String,
    _api_key: String,
    _model: String,
    label: String,
}

impl HttpBackend {
    pub fn new(endpoint: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let endpoint = endpoint.into();
        let model = model.into();
        let label = format!("{model} via http");
        Self {
            _endpoint: endpoint,
            _api_key: api_key.into(),
            _model: model,
            label,
        }
    }
}

#[async_trait]
impl Backend for HttpBackend {
    async fn generate(&self, _prompt: &str) -> anyhow::Result<String> {
        anyhow::bail!("HttpBackend not yet implemented")
    }

    async fn generate_json(&self, _prompt: &str) -> anyhow::Result<String> {
        anyhow::bail!("HttpBackend not yet implemented")
    }

    fn model_label(&self) -> &str {
        &self.label
    }
}
