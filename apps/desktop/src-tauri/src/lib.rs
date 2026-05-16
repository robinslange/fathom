use fathom_core::{fathom, FathomResult, Mode, Tier};
use fathom_engine::OllamaBackend;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ParaphraseArgs {
    pub text: String,
    pub tier: Tier,
    pub mode: Mode,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParaphraseError {
    pub message: String,
}

impl From<anyhow::Error> for ParaphraseError {
    fn from(e: anyhow::Error) -> Self {
        Self {
            message: format!("{e:#}"),
        }
    }
}

#[tauri::command]
async fn paraphrase(args: ParaphraseArgs) -> Result<FathomResult, ParaphraseError> {
    let model = args.model.unwrap_or_else(|| "gemma3:4b".to_string());
    let mut backend = OllamaBackend::new(model);
    if let Some(url) = args.base_url {
        backend = backend.with_base_url(url);
    }
    Ok(fathom(args.text, args.tier, args.mode, &backend).await?)
}

#[tauri::command]
fn lexicon_stats() -> LexiconStats {
    use std::collections::BTreeSet;
    let entries = fathom_core::lexicon::all_entries();
    let mut traditions = BTreeSet::new();
    let mut authors = BTreeSet::new();
    for e in entries {
        if !e.source.tradition.is_empty() {
            traditions.insert(e.source.tradition.clone());
        }
        authors.insert(e.source.author.clone());
    }
    LexiconStats {
        passages: entries.len(),
        traditions: traditions.into_iter().collect(),
        authors: authors.into_iter().collect(),
    }
}

#[derive(Serialize)]
pub struct LexiconStats {
    pub passages: usize,
    pub traditions: Vec<String>,
    pub authors: Vec<String>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![paraphrase, lexicon_stats])
        .run(tauri::generate_context!())
        .expect("error while running fathom desktop");
}
