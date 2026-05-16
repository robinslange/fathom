//! Stage modules. Each owns its CLI args, IO contract, and implementation.

pub mod catalog_sync;
pub mod chunk_stage;
pub mod embed_stage;
pub mod enrich_translators;
pub mod fetch_corpus;
pub mod filter_stage;
pub mod harvest_substrate;
pub mod manifest;
pub mod shard;
pub mod sign;
pub mod verify;
