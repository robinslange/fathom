use crate::prompts::{render, IDENTIFY_PROMPT};
use fathom_engine::Backend;
use serde::Deserialize;

#[derive(Deserialize)]
struct IdentifyResponse {
    #[serde(default)]
    terms: Vec<String>,
}

/// JIT pass 1: ask the model to identify English phrases that translate
/// technical philosophical concepts. Empty list on parse failure — the
/// caller falls through to `no-substrate` glossing.
pub async fn identify_terms(passage: &str, backend: &dyn Backend) -> anyhow::Result<Vec<String>> {
    let prompt = render(IDENTIFY_PROMPT, &[("passage", passage)]);
    let response = backend.generate_json(&prompt).await?;
    let parsed: Result<IdentifyResponse, _> = serde_json::from_str(&response);
    Ok(parsed
        .map(|p| {
            p.terms
                .into_iter()
                .filter(|t| !t.trim().is_empty())
                .collect()
        })
        .unwrap_or_default())
}
