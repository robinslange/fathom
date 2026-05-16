//! Fathom v0.2 corpus build pipeline.
//!
//! Operator-only tool. Runs on Robin's machine. Produces signed manifest +
//! per-book msgpack-zstd shards in ./dist/, ready for separate R2 deploy.
//!
//! Stages (each idempotent, content-addressed where reasonable):
//!   catalog-sync       Fetch pg_catalog.csv, filter to LoCC=B*, Language=en
//!   enrich-translators For each candidate: parse pg{id}.rdf for <marcrel:trl>,
//!                      fall back to Wikidata + Open Library for missing
//!   filter             Apply NZ life+50 cutoff (translator d. ≤ 1975), exclude unresolved
//!   fetch-corpus       rsync EPUB subset via --files-from from filtered IDs
//!   chunk              Parse EPUBs (rbook + roxmltree), chunk via fathom-chunker
//!   embed              Pass chunks through fathom-embed (bge-small CPU)
//!   shard              Pack per-book {chunks + embeddings + offsets} → msgpack-zstd
//!   manifest           Assemble index.msgpack — books, traditions (from traditions.json),
//!                      per-shard SHA-256
//!   sign               minisign on index.msgpack → index.msgpack.minisig
//!   all                Run the full pipeline
//!   verify             Sanity-check a built ./dist/ tree (hashes + signature)
//!
//! Deploy is a separate step. See decision note for rclone invocation.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "fathom-build")]
#[command(about = "Fathom v0.2 corpus build pipeline (operator tool)")]
struct Cli {
    #[command(subcommand)]
    command: Stage,
}

#[derive(Subcommand)]
enum Stage {
    /// Fetch pg_catalog.csv and filter to LoCC=B* English-language candidates.
    CatalogSync,
    /// Parse per-book RDFs for translator metadata; fall back to Wikidata + Open Library.
    EnrichTranslators,
    /// Apply NZ life+50 cutoff; default-exclude unresolved.
    Filter,
    /// rsync the filtered EPUB subset from rsync.ibiblio.org::gutenberg-epub.
    FetchCorpus,
    /// Parse EPUBs into paragraph chunks via fathom-chunker.
    Chunk,
    /// Embed all chunks via fathom-embed (bge-small CPU).
    Embed,
    /// Pack per-book shards (msgpack-zstd).
    Shard,
    /// Assemble the manifest (index.msgpack).
    Manifest,
    /// Sign the manifest via minisign.
    Sign,
    /// Run the full pipeline end-to-end.
    All,
    /// Verify a built dist tree (hashes, signature, manifest schema).
    Verify,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Stage::CatalogSync => stage_catalog_sync().await,
        Stage::EnrichTranslators => stage_enrich_translators().await,
        Stage::Filter => stage_filter().await,
        Stage::FetchCorpus => stage_fetch_corpus().await,
        Stage::Chunk => stage_chunk().await,
        Stage::Embed => stage_embed().await,
        Stage::Shard => stage_shard().await,
        Stage::Manifest => stage_manifest().await,
        Stage::Sign => stage_sign().await,
        Stage::All => stage_all().await,
        Stage::Verify => stage_verify().await,
    }
}

async fn stage_catalog_sync() -> anyhow::Result<()> {
    todo!("fetch https://www.gutenberg.org/cache/epub/feeds/pg_catalog.csv, filter LoCC + Language, write build-state/candidates.json")
}

async fn stage_enrich_translators() -> anyhow::Result<()> {
    todo!("for each candidate: fetch pg{{id}}.rdf, extract <marcrel:trl>; on miss, queue for Wikidata + Open Library")
}

async fn stage_filter() -> anyhow::Result<()> {
    todo!("apply translator d. ≤ 1975; default-exclude unresolved; write build-state/filtered.json")
}

async fn stage_fetch_corpus() -> anyhow::Result<()> {
    todo!("invoke rsync --files-from=ids.txt against rsync.ibiblio.org::gutenberg-epub into build-state/corpus/")
}

async fn stage_chunk() -> anyhow::Result<()> {
    todo!("for each EPUB: rbook spine iteration, roxmltree paragraph extraction with HTML-entity pre-process, fathom-chunker, write build-state/chunks/{{id}}.json")
}

async fn stage_embed() -> anyhow::Result<()> {
    todo!("for each book's chunks: fathom-embed batch (32-64 at a time), write build-state/embeddings/{{id}}.bin")
}

async fn stage_shard() -> anyhow::Result<()> {
    todo!("for each book: combine chunks + embeddings + offsets → rmp-serde + zstd, write dist/shards/{{id}}.shard")
}

async fn stage_manifest() -> anyhow::Result<()> {
    todo!("assemble index.msgpack from filtered.json + traditions.json + per-shard SHA-256; write dist/index.msgpack")
}

async fn stage_sign() -> anyhow::Result<()> {
    todo!("minisign sign dist/index.msgpack → dist/index.msgpack.minisig using ~/.minisign/minisign.key")
}

async fn stage_all() -> anyhow::Result<()> {
    stage_catalog_sync().await?;
    stage_enrich_translators().await?;
    stage_filter().await?;
    stage_fetch_corpus().await?;
    stage_chunk().await?;
    stage_embed().await?;
    stage_shard().await?;
    stage_manifest().await?;
    stage_sign().await?;
    Ok(())
}

async fn stage_verify() -> anyhow::Result<()> {
    todo!("recompute SHA-256 over each shard, compare to manifest; verify minisig against dist/fathom.pub")
}
