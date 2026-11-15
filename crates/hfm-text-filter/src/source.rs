//! Text source abstraction.
//! Sources produce raw chunks of text from a document (file, PDF, etc.).

use crate::types::{Offset, RawChunk, ReaderError, ReaderResult};
use std::time::Duration;

pub mod local;

/// Outcome of a `try_pull_chunk` operation.
#[derive(Debug, Clone, PartialEq)]
pub enum PullOutcome {
    /// A raw chunk was successfully pulled.
    Chunk(RawChunk),
    /// No chunk was available within the timeout.
    Empty,
    /// The source has reached the end of the document.
    Eos,
}

/// A source of raw text chunks.
///
/// This trait abstracts over different document formats (plain text, PDF, web).
/// Implementations must be thread-safe (`Send + Sync`) as they are used from
/// the pipeline worker thread.
pub trait TextSource: Send + Sync {
    /// Try to pull the next raw chunk within the given timeout.
    ///
    /// If a chunk is available, returns `PullOutcome::Chunk`.
    /// If no chunk arrives within `timeout`, returns `PullOutcome::Empty`.
    /// If the source has reached the end of the document, returns `PullOutcome::Eos`.
    fn try_pull_chunk(&mut self, timeout: Duration) -> PullOutcome;

    /// Seek to a specific byte offset in the source document.
    ///
    /// The offset is an absolute byte offset from the start of the document.
    /// After a successful seek, the next call to `try_pull_chunk` will return
    /// chunks starting from or after this offset.
    ///
    /// # Errors
    /// Returns `ReaderError::Seek` if the offset is invalid or the source
    /// does not support seeking.
    fn seek(&mut self, offset: Offset) -> ReaderResult<()>;
}

// Re-export the local implementation.
pub use local::LocalFileSource;