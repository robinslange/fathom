use crate::Backend;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";
pub const DEFAULT_TEMPERATURE: f32 = 0.2;
pub const DEFAULT_NUM_PREDICT: u32 = 2000;

pub struct OllamaBackend {
    model: String,
    base_url: String,
    temperature: f32,
    label: String,
    client: reqwest::Client,
}

impl OllamaBackend {
    pub fn new(model: impl Into<String>) -> Self {
        let model = model.into();
        let label = format!("{model} via ollama");
        Self {
            model,
            base_url: resolve_base_url(None),
            temperature: DEFAULT_TEMPERATURE,
            label,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    async fn call(&self, prompt: &str, json_format: bool) -> anyhow::Result<String> {
        let payload = GenerateRequest {
            model: &self.model,
            prompt,
            stream: false,
            options: Options {
                temperature: self.temperature,
                num_predict: DEFAULT_NUM_PREDICT,
            },
            think: false,
            format: if json_format { Some("json") } else { None },
        };
        let response = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;
        let parsed: GenerateResponse = response.json().await?;
        Ok(parsed.response)
    }
}

#[async_trait]
impl Backend for OllamaBackend {
    async fn generate(&self, prompt: &str) -> anyhow::Result<String> {
        self.call(prompt, false).await
    }

    async fn generate_json(&self, prompt: &str) -> anyhow::Result<String> {
        self.call(prompt, true).await
    }

    fn model_label(&self) -> &str {
        &self.label
    }
}

/// Resolve the Ollama base URL: explicit override > env var > default.
pub fn resolve_base_url(override_url: Option<String>) -> String {
    override_url
        .or_else(|| std::env::var("FATHOM_OLLAMA_URL").ok())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    options: Options,
    think: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a str>,
}

#[derive(Serialize)]
struct Options {
    temperature: f32,
    num_predict: u32,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}
