//! Wire format for per-book shards. msgpack-serialised, zstd-compressed.
//!
//! A shard is the unit of fetch on the runtime side: one HTTP GET per book,
//! contains everything needed to render the book + run kNN over its chunks.
//!
//! `embedding_f16` is a packed 384-dim f16 vector, 768 bytes per chunk, in
//! chunk-order. The runtime unpacks via `fathom_embed::from_f16_bytes`.

use crate::types::Agent;
use fathom_chunker::Chunk;
use serde::{Deserialize, Serialize};

/// Wire-format version. Bump on any breaking change to the shard schema.
pub const SHARD_FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shard {
    pub format_version: u32,
    pub gutenberg_id: u32,
    pub title: String,
    pub translators: Vec<Agent>,
    pub embed_model_id: String,
    /// Full canonical text — same string the chunker indexed against. The
    /// runtime reader renders from this; offsets in `chunks` are positions
    /// into this string. Compressed by zstd before transport.
    pub canonical_text: String,
    pub chunks: Vec<ShardChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardChunk {
    pub chunk_id: String,
    pub paragraph_id: String,
    pub section_id: Option<String>,
    pub byte_offset_start: usize,
    pub byte_offset_end: usize,
    pub token_count: usize,
    /// 768 bytes: 384-dim f16 vector.
    #[serde(with = "serde_bytes")]
    pub embedding_f16: Vec<u8>,
}

impl ShardChunk {
    pub fn from_chunk(chunk: &Chunk, embedding_f16: Vec<u8>) -> Self {
        debug_assert_eq!(embedding_f16.len(), 768, "expected 384 dims × 2 bytes");
        ShardChunk {
            chunk_id: chunk.chunk_id.clone(),
            paragraph_id: chunk.paragraph_id.clone(),
            section_id: chunk.section_id.clone(),
            byte_offset_start: chunk.byte_offset_start,
            byte_offset_end: chunk.byte_offset_end,
            token_count: chunk.token_count,
            embedding_f16,
        }
    }
}
