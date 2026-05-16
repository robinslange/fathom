use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand, ValueEnum};
use fathom_core::{bootstrap, fathom, Mode, Tier};
use fathom_engine::{Backend, LlamaCppBackend, OllamaBackend};
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "fathom", version, about = "Paraphrase classical philosophy, preserving terms-of-art")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Paraphrase a philosophy passage from FILE (use - for stdin).
    Paraphrase {
        /// Input file path, or `-` for stdin.
        file: String,

        /// Audience tier.
        #[arg(long, value_enum, default_value_t = TierArg::Standard)]
        tier: TierArg,

        /// Substrate resolution strategy.
        #[arg(long, value_enum, default_value_t = ModeArg::Auto)]
        mode: ModeArg,

        /// Inference backend.
        #[arg(long, value_enum, default_value_t = BackendArg::Ollama)]
        backend: BackendArg,

        /// Ollama model tag (only used when --backend=ollama).
        #[arg(long, default_value = "gemma3:4b")]
        model: String,

        /// Ollama base URL. Falls back to $FATHOM_OLLAMA_URL then http://localhost:11434.
        #[arg(long)]
        base_url: Option<String>,

        /// Manifest model id for the bundled llama.cpp backend (only used when --backend=llama-cpp).
        #[arg(long, default_value = "gemma3-4b")]
        llama_model: String,

        /// Emit JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Show how many lexicon entries are loaded and which traditions are covered.
    Lexicon,
    /// Download a model file into the OS app data dir if not already present.
    Bootstrap {
        /// Manifest model id (e.g. `gemma3-4b`, `deberta-nli`).
        #[arg(long, default_value = "gemma3-4b")]
        model: String,
    },
}

#[derive(ValueEnum, Clone, Copy)]
enum TierArg {
    Simple,
    Standard,
    Scholarly,
}

impl From<TierArg> for Tier {
    fn from(t: TierArg) -> Self {
        match t {
            TierArg::Simple => Tier::Simple,
            TierArg::Standard => Tier::Standard,
            TierArg::Scholarly => Tier::Scholarly,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
enum ModeArg {
    Auto,
    Curated,
    Jit,
    NoSubstrate,
}

impl From<ModeArg> for Mode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::Auto => Mode::Auto,
            ModeArg::Curated => Mode::Curated,
            ModeArg::Jit => Mode::Jit,
            ModeArg::NoSubstrate => Mode::NoSubstrate,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
enum BackendArg {
    Ollama,
    LlamaCpp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Paraphrase {
            file,
            tier,
            mode,
            backend,
            model,
            base_url,
            llama_model,
            json,
        } => {
            let text = read_input(&file)?;
            let backend_impl: Box<dyn Backend> = match backend {
                BackendArg::Ollama => {
                    let mut b = OllamaBackend::new(&model);
                    if let Some(url) = base_url {
                        b = b.with_base_url(url);
                    }
                    Box::new(b)
                }
                BackendArg::LlamaCpp => {
                    let path = bootstrap::ensure_model_downloaded(
                        &llama_model,
                        Some(stderr_progress()),
                    )
                    .await?;
                    eprintln!("\nloading model from {} ...", path.display());
                    Box::new(LlamaCppBackend::load(path)?)
                }
            };
            let result = fathom(text, tier.into(), mode.into(), backend_impl.as_ref()).await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!(
                    "\nresolution: {:?} · tier: {:?} · model: {}\n",
                    result.resolution, result.tier, result.model
                );
                println!("PARAPHRASE:\n{}\n", result.paraphrase);
                if !result.glossary.is_empty() {
                    println!("GLOSSARY:");
                    for entry in &result.glossary {
                        let sub = entry
                            .substrate_term
                            .as_deref()
                            .map(|s| format!(" (`{s}`)"))
                            .unwrap_or_default();
                        println!("  - {}{}: {}", entry.term, sub, entry.gloss);
                    }
                }
            }
        }
        Command::Lexicon => {
            use std::collections::BTreeSet;
            let entries = fathom_core::lexicon::all_entries();
            println!("lexicon: {} passages embedded", entries.len());
            let mut traditions: BTreeSet<&str> = BTreeSet::new();
            let mut authors: BTreeSet<&str> = BTreeSet::new();
            for e in entries {
                if !e.source.tradition.is_empty() {
                    traditions.insert(&e.source.tradition);
                }
                authors.insert(&e.source.author);
            }
            println!("traditions: {}", traditions.into_iter().collect::<Vec<_>>().join(", "));
            println!("authors: {}", authors.into_iter().collect::<Vec<_>>().join(", "));
        }
        Command::Bootstrap { model } => {
            let entry = bootstrap::lookup_manifest(&model)
                .ok_or_else(|| anyhow!("unknown model id: {model}"))?;
            eprintln!(
                "downloading {} ({} MB est.) from {}",
                entry.label,
                entry.size_estimate_bytes / 1_000_000,
                entry.url
            );
            let path = bootstrap::ensure_model_downloaded(&model, Some(stderr_progress())).await?;
            eprintln!("\nready: {}", path.display());
        }
    }
    Ok(())
}

fn read_input(file: &str) -> Result<String> {
    if file == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        Ok(std::fs::read_to_string(PathBuf::from(file))?)
    }
}

fn stderr_progress() -> bootstrap::ProgressCallback {
    use std::io::Write;
    Box::new(|bytes, total| {
        let bytes_mb = bytes / 1_000_000;
        match total {
            Some(t) => {
                let total_mb = t / 1_000_000;
                eprint!("\r  {} / {} MB", bytes_mb, total_mb);
            }
            None => eprint!("\r  {} MB", bytes_mb),
        }
        let _ = std::io::stderr().flush();
    })
}
