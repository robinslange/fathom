pub mod identify;
pub mod judge;
pub mod lexicon;
pub mod orchestrate;
pub mod parser;
pub mod prompts;
pub mod types;

pub use orchestrate::fathom;
pub use types::{
    FaithfulnessScore, FathomResult, GlossaryEntry, Mode, Passage, Resolution, Tier,
};
