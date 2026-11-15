//! Text filter abstraction and implementations.

pub mod mock;
pub mod detox;

use crate::types::{ProcessedChunk, RawChunk, ReaderResult};

/// A filter that processes raw text chunks.
///
/// Implementations are expected to be stateless or internally synchronised,
/// as they may be called concurrently from the ML worker thread.
pub trait TextFilter: Send + Sync {
    /// Process a raw chunk and produce a processed chunk.
    fn process(&self, raw: &RawChunk) -> ReaderResult<ProcessedChunk>;
}

// Re-export filters.
pub use mock::{UppercaseFilter, NoopFilter};
pub use detox::BartDetoxFilter;
