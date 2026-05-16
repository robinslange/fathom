//! Fathom v0.2 corpus build pipeline.
//!
//! Operator-only tool. Runs on Robin's machine. Produces signed manifest +
//! per-book msgpack-zstd shards in ./dist/, ready for separate R2 deploy.
//!
//! See `0-inbox/fathom-v0-2-build-pipeline-shape-decision.md` for architecture.

use clap::Parser;

mod catalog;
mod fs_state;
mod stages;
mod translators;
mod types;

use stages::{
    catalog_sync, chunk_stage, embed_stage, enrich_translators, fetch_corpus, filter_stage,
    manifest, shard, sign, verify,
};

#[derive(Parser)]
#[command(name = "fathom-build")]
#[command(about = "Fathom v0.2 corpus build pipeline (operator tool)")]
struct Cli {
    #[command(subcommand)]
    command: Stage,
}

#[derive(clap::Subcommand)]
enum Stage {
    /// Fetch pg_catalog.csv and filter to LoCC=B* English-language candidates.
    CatalogSync(catalog_sync::Args),
    /// Parse per-book RDFs for translator metadata; fall back to Wikidata + Open Library.
    EnrichTranslators(enrich_translators::Args),
    /// Apply NZ life+50 cutoff; default-exclude unresolved.
    Filter(filter_stage::Args),
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
        Stage::CatalogSync(args) => catalog_sync::run(args).await,
        Stage::EnrichTranslators(args) => enrich_translators::run(args).await,
        Stage::Filter(args) => filter_stage::run(args).await,
        Stage::FetchCorpus => fetch_corpus::run().await,
        Stage::Chunk => chunk_stage::run().await,
        Stage::Embed => embed_stage::run().await,
        Stage::Shard => shard::run().await,
        Stage::Manifest => manifest::run().await,
        Stage::Sign => sign::run().await,
        Stage::All => {
            catalog_sync::run(catalog_sync::Args::default()).await?;
            enrich_translators::run(enrich_translators::Args::default()).await?;
            filter_stage::run(filter_stage::Args::default()).await?;
            fetch_corpus::run().await?;
            chunk_stage::run().await?;
            embed_stage::run().await?;
            shard::run().await?;
            manifest::run().await?;
            sign::run().await?;
            Ok(())
        }
        Stage::Verify => verify::run().await,
    }
}
