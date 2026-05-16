pub mod bootstrap;
pub mod identify;
pub mod judge;
pub mod lexicon;
pub mod library;
pub mod orchestrate;
pub mod parser;
pub mod prompts;
pub mod types;

pub use orchestrate::{fathom, fathom_with_judge, JudgeMode};
pub use types::{
    FaithfulnessScore, FaithfulnessVerdict, FathomResult, GlossaryEntry, Mode, Passage, Resolution,
    Tier,
};
