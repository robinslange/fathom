use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use fathom_core::{fathom, Mode, Tier};
use fathom_engine::OllamaBackend;
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

        /// Ollama model tag.
        #[arg(long, default_value = "gemma3:4b")]
        model: String,

        /// Ollama base URL. Falls back to $FATHOM_OLLAMA_URL then http://localhost:11434.
        #[arg(long)]
        base_url: Option<String>,

        /// Emit JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Show how many lexicon entries are loaded and which traditions are covered.
    Lexicon,
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Paraphrase {
            file,
            tier,
            mode,
            model,
            base_url,
            json,
        } => {
            let text = read_input(&file)?;
            let mut backend = OllamaBackend::new(&model);
            if let Some(url) = base_url {
                backend = backend.with_base_url(url);
            }
            let result = fathom(text, tier.into(), mode.into(), &backend).await?;

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
