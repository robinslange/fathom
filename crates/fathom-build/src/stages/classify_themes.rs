//! Stage 12 — classify-themes.
//!
//! Operator-only stage. For each book in the corpus manifest, dispatches
//! a Sonnet 4.6 subagent (via the host Claude Code session) with the book's
//! title + first-page text + a beginner-framed theme taxonomy; assembles
//! the returned classifications into `crates/fathom-core/data/themes.json`
//! for in-binary inclusion.
//!
//! The actual subagent dispatch is driven by the host session, not by this
//! binary — this stage prepares the per-book input JSON
//! (`build-state/themes-input.jsonl`) and consumes the agent-produced
//! output JSON (`build-state/themes-output.jsonl`), then assembles the
//! final `themes.json`.

#[allow(unused_imports)]
use crate::fs_state::{build_state_dir, read_json, write_json};
#[allow(unused_imports)]
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
#[allow(unused_imports)]
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Limit to first N books — useful for the 50-book validation pass.
    #[arg(long)]
    pub limit: Option<usize>,
    /// Skip the prepare step (input JSONL already written by a previous run).
    #[arg(long, default_value_t = false)]
    pub skip_prepare: bool,
    /// Skip the assemble step (only emit the input JSONL).
    #[arg(long, default_value_t = false)]
    pub skip_assemble: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            limit: None,
            skip_prepare: false,
            skip_assemble: false,
        }
    }
}

pub async fn run(args: Args) -> Result<()> {
    eprintln!("classify-themes: limit={:?}", args.limit);
    if !args.skip_prepare {
        prepare_input(args.limit)?;
    }
    if !args.skip_assemble {
        assemble_output()?;
    }
    Ok(())
}

fn prepare_input(_limit: Option<usize>) -> Result<()> {
    // Implemented in Task 2.
    anyhow::bail!("prepare_input not yet implemented")
}

fn assemble_output() -> Result<()> {
    // Implemented in Task 3.
    anyhow::bail!("assemble_output not yet implemented")
}
