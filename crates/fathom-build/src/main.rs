//! Fathom v0.2 corpus build pipeline.
//!
//! Operator-only tool. Runs on Robin's machine. Produces signed manifest +
//! per-book msgpack-zstd shards in ./dist/, ready for separate R2 deploy.
//!
//! See `0-inbox/fathom-v0-2-build-pipeline-shape-decision.md` for architecture.

use clap::Parser;

mod catalog;
mod fs_state;
mod shard_format;
mod stages;
mod translators;
mod types;

use stages::{
    catalog_sync, chunk_stage, classify_themes, deploy, embed_stage, enrich_translators,
    fetch_corpus, filter_stage, harvest_substrate, manifest, shard, sign, verify,
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
    FetchCorpus(fetch_corpus::Args),
    /// Parse EPUBs into paragraph chunks via fathom-chunker.
    Chunk(chunk_stage::Args),
    /// Embed all chunks via fathom-embed (bge-small CPU).
    Embed(embed_stage::Args),
    /// Pack per-book shards (msgpack-zstd).
    Shard(shard::Args),
    /// Assemble the manifest (index.msgpack).
    Manifest(manifest::Args),
    /// Sign the manifest via minisign.
    Sign(sign::Args),
    /// Harvest substrate-term candidates from the chunked corpus into
    /// pending-lexicon.jsonl for operator review.
    HarvestSubstrate(harvest_substrate::Args),
    /// Classify books into beginner-facing themes via Sonnet 4.6 subagents.
    /// Operator-driven; input/output JSONL passes through build-state/.
    ClassifyThemes(classify_themes::Args),
    /// Upload the signed dist tree to R2 via the locally authenticated wrangler CLI.
    Deploy(deploy::Args),
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
        Stage::FetchCorpus(args) => fetch_corpus::run(args).await,
        Stage::Chunk(args) => chunk_stage::run(args).await,
        Stage::Embed(args) => embed_stage::run(args).await,
        Stage::Shard(args) => shard::run(args).await,
        Stage::Manifest(args) => manifest::run(args).await,
        Stage::Sign(args) => sign::run(args).await,
        Stage::HarvestSubstrate(args) => harvest_substrate::run(args).await,
        Stage::ClassifyThemes(args) => classify_themes::run(args).await,
        Stage::Deploy(args) => deploy::run(args).await,
        Stage::All => {
            catalog_sync::run(catalog_sync::Args::default()).await?;
            enrich_translators::run(enrich_translators::Args::default()).await?;
            filter_stage::run(filter_stage::Args::default()).await?;
            fetch_corpus::run(fetch_corpus::Args::default()).await?;
            chunk_stage::run(chunk_stage::Args::default()).await?;
            embed_stage::run(embed_stage::Args {
                model_dir: std::env::var("FATHOM_BGE_MODEL_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default(),
                batch: 32,
                limit: None,
                force: false,
            })
            .await?;
            shard::run(shard::Args::default()).await?;
            manifest::run(manifest::Args::default()).await?;
            sign::run(sign::Args {
                key: None,
                pub_key: None,
                auto_generate: true,
            })
            .await?;
            Ok(())
        }
        Stage::Verify => verify::run().await,
    }
}
