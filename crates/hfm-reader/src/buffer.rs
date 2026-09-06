//! Text buffer abstraction.
//! The buffer stores raw and processed chunks, handles offset shifting, and
//! provides a contiguous view of the processed document.

use crate::types::{Offset, RawChunk, ProcessedChunk, ReaderError, ReaderResult};

/// A buffer that manages the document's raw and processed state.
///
/// The buffer maintains a contiguous `String` representing the fully processed
/// document. When a processed chunk arrives, it replaces the corresponding raw
/// range and shifts all subsequent offsets.
///
/// It also tracks the original offsets of raw chunks to support seeking.
pub trait TextBuffer: Send + Sync {
    /// Insert a raw chunk at its original offset.
    ///
    /// The buffer stores the raw data and marks the range as "pending" (not yet processed).
    /// If a processed chunk for the same offset already exists, this operation may
    /// be ignored or overwritten depending on the implementation.
    ///
    /// # Errors
    /// Returns `ReaderError::BufferFull` if the buffer capacity is exceeded.
    fn insert_raw(&mut self, chunk: RawChunk) -> ReaderResult<()>;

    /// Apply a processed chunk, replacing the raw data at the same offset.
    ///
    /// The buffer will locate the raw chunk that starts at `chunk.offset`,
    /// replace its data with `chunk.data`, and shift all subsequent chunks
    /// by `(new_len - old_len)`.
    ///
    /// If no raw chunk exists at that exact offset, this operation fails.
    ///
    /// # Errors
    /// Returns `ReaderError::Internal` if the raw chunk is not found.
    fn apply_processed(&mut self, chunk: ProcessedChunk) -> ReaderResult<()>;

    /// Read a contiguous range of the **processed** document.
    ///
    /// The range is defined in the current (shifted) coordinate space.
    ///
    /// # Errors
    /// Returns `ReaderError::OffsetOutOfBounds` if the range is invalid.
    fn read_range(&self, from: Offset, to: Offset) -> ReaderResult<String>;

    /// Translate a current offset (in the processed document) to the
    /// corresponding original raw offset.
    ///
    /// This is used when the user seeks to a position in the processed document;
    /// we need to tell the `TextSource` where to resume reading raw data.
    ///
    /// # Errors
    /// Returns `ReaderError::OffsetOutOfBounds` if the offset is outside
    /// the processed document.
    fn translate_to_raw(&self, processed_offset: Offset) -> ReaderResult<Offset>;

    /// Discard all chunks that end **before** the given offset.
    ///
    /// Chunks that overlap or start after `offset` are retained.
    /// This is used to bound memory usage when the user scrolls forward.
    ///
    /// # Parameters
    /// - `threshold`: the processed offset before which chunks are discarded.
    /// - `generation`: the current seek generation; chunks from older generations
    ///   are also discarded regardless of offset.
    ///
    /// Returns the discarded raw chunks (for potential cleanup).
    fn flush_before(&mut self, threshold: Offset, generation: u64) -> Vec<RawChunk>;
}