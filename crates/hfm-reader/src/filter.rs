//! Text filter trait.
//! Filters transform raw text chunks into processed text chunks.

use crate::types::{ProcessedChunk, RawChunk, ReaderError, ReaderResult};

/// A filter that processes raw text chunks.
///
/// Implementations are expected to be stateless or internally synchronised,
/// as they may be called concurrently from the ML worker thread.
pub trait TextFilter: Send + Sync {
    /// Process a raw chunk and produce a processed chunk.
    ///
    /// The `ProcessedChunk` must have the same `offset` as the input `RawChunk`.
    /// The `data` field may have a different length (shorter or longer).
    ///
    /// # Errors
    /// Returns `ReaderError::Filter` if processing fails (e.g., model error).
    fn process(&self, raw: &RawChunk) -> ReaderResult<ProcessedChunk>;
}