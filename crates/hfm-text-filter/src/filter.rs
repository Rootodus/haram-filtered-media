//! Text filter abstraction and implementations.

pub mod mock;
pub mod detox;

use crate::types::{ProcessedChunk, RawChunk, ReaderResult};

/// A filter that processes raw text chunks.
pub trait TextFilter: Send + Sync {
    /// Process a raw chunk and produce a processed chunk.
    fn process(&self, raw: &RawChunk) -> ReaderResult<ProcessedChunk>;
}

// Re-export filters.
pub use mock::{UppercaseFilter, NoopFilter};
pub use detox::BartDetoxFilter;