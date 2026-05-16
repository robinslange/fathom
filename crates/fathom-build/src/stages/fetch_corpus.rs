//! Stage 4 — fetch-corpus.
//!
//! Build a `--files-from` list from filtered.json and rsync the EPUB subset
//! from `rsync.ibiblio.org::gutenberg-epub` into `build-state/corpus/`.
//!
//! Layout in the source rsync module is flat: `{id}/pg{id}.epub` (the
//! `gutenberg-epub` module's content starts at the bare ID directory, not
//! the hierarchical 1/2/3/4 layout used by the main `::gutenberg` module).
//!
//! Idempotent: re-runs are cheap (rsync skips unchanged files via mtime+size).
//! The list of IDs to fetch is rebuilt every invocation from filtered.json so
//! adding/removing books is automatically reflected.

use crate::fs_state::{build_state_dir, ensure_dir, filtered_path, read_json};
use crate::types::Filtered;
use anyhow::{bail, Context, Result};
use clap::Args as ClapArgs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

const RSYNC_ENDPOINT: &str = "rsync.ibiblio.org::gutenberg-epub";

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    /// Limit to the first N books (for smoke runs).
    #[arg(long)]
    pub limit: Option<usize>,
    /// Skip rsync; assume corpus already present on disk.
    #[arg(long)]
    pub skip_rsync: bool,
    /// Override the rsync endpoint (for testing against a local mirror).
    #[arg(long)]
    pub rsync_endpoint: Option<String>,
}

pub async fn run(args: Args) -> Result<()> {
    let filtered: Vec<Filtered> =
        read_json(&filtered_path()).context("load filtered.json — run filter first")?;

    let target: Vec<&Filtered> = match args.limit {
        Some(n) => filtered.iter().take(n).collect(),
        None => filtered.iter().collect(),
    };

    eprintln!("fetch-corpus: {} books to fetch", target.len());

    let corpus_dir = build_state_dir().join("corpus");
    ensure_dir(&corpus_dir)?;

    if args.skip_rsync {
        eprintln!("fetch-corpus: --skip-rsync set; not invoking rsync");
        return Ok(());
    }

    let list_path = build_state_dir().join("rsync-files.txt");
    write_file_list(&list_path, &target)?;
    eprintln!("fetch-corpus: file list at {}", list_path.display());

    let endpoint = args.rsync_endpoint.unwrap_or_else(|| RSYNC_ENDPOINT.to_string());
    invoke_rsync(&list_path, &corpus_dir, &endpoint)?;

    let found = count_fetched(&corpus_dir, &target)?;
    eprintln!(
        "fetch-corpus: {}/{} EPUBs present after rsync",
        found,
        target.len()
    );
    Ok(())
}

fn write_file_list(path: &PathBuf, books: &[&Filtered]) -> Result<()> {
    let mut f = std::fs::File::create(path)
        .with_context(|| format!("create file list {}", path.display()))?;
    for b in books {
        // Layout: `{id}/pg{id}.epub` relative to gutenberg-epub module root.
        writeln!(f, "{}/pg{}.epub", b.gutenberg_id, b.gutenberg_id)?;
    }
    Ok(())
}

fn invoke_rsync(list_path: &PathBuf, dest: &PathBuf, endpoint: &str) -> Result<()> {
    // -rt: recursive + preserve mtime. -v: verbose.
    // --no-perms / --no-owner / --no-group: don't attempt chmod/chown on the
    // local copy — macOS sandbox + non-root users can't replicate the source's
    // owner/perms, and we don't care about them for read-only EPUBs.
    let status = Command::new("rsync")
        .arg("-rtv")
        .arg("--no-perms")
        .arg("--no-owner")
        .arg("--no-group")
        .arg("--files-from")
        .arg(list_path)
        .arg("--timeout=600")
        .arg(endpoint)
        .arg(dest)
        .status()
        .context("invoke rsync — is it installed?")?;
    if !status.success() {
        bail!("rsync exited with {:?}", status.code());
    }
    Ok(())
}

fn count_fetched(corpus_dir: &PathBuf, books: &[&Filtered]) -> Result<usize> {
    let mut found = 0;
    for b in books {
        let path = corpus_dir.join(format!("{}/pg{}.epub", b.gutenberg_id, b.gutenberg_id));
        if path.is_file() {
            found += 1;
        }
    }
    Ok(found)
}
