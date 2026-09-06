//! Core data types and errors for the text processing pipeline.

use std::fmt;

/// Represents a byte offset in the source document.
/// Offsets are always in UTF-8 bytes, not characters or grapheme clusters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Offset(pub u64);

impl Offset {
    pub const ZERO: Self = Offset(0);
    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<usize> for Offset {
    fn from(v: usize) -> Self {
        Offset(v as u64)
    }
}

impl From<u64> for Offset {
    fn from(v: u64) -> Self {
        Offset(v)
    }
}

/// A raw (unprocessed) text chunk with its starting offset.
#[derive(Debug, Clone, PartialEq)]
pub struct RawChunk {
    pub offset: Offset,
    pub data: String,
}

impl RawChunk {
    pub fn new(offset: Offset, data: impl Into<String>) -> Self {
        Self {
            offset,
            data: data.into(),
        }
    }

    /// Returns the end offset (exclusive) of this chunk.
    pub fn end_offset(&self) -> Offset {
        Offset(self.offset.0 + self.data.len() as u64)
    }
}

/// A processed text chunk with its starting offset.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedChunk {
    pub offset: Offset,
    pub data: String,
}

impl ProcessedChunk {
    pub fn new(offset: Offset, data: impl Into<String>) -> Self {
        Self {
            offset,
            data: data.into(),
        }
    }

    pub fn end_offset(&self) -> Offset {
        Offset(self.offset.0 + self.data.len() as u64)
    }
}

/// The state of a chunk within the buffer.
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkState {
    /// Raw data waiting to be processed.
    Raw(RawChunk),
    /// Currently being processed by the ML worker.
    Processing,
    /// Processed data ready for display.
    Ready(ProcessedChunk),
}

impl ChunkState {
    /// Returns the offset of the chunk if it is Raw or Ready.
    pub fn offset(&self) -> Option<Offset> {
        match self {
            ChunkState::Raw(c) => Some(c.offset),
            ChunkState::Ready(c) => Some(c.offset),
            ChunkState::Processing => None,
        }
    }

    /// Returns the data if available (Raw or Ready).
    pub fn data(&self) -> Option<&str> {
        match self {
            ChunkState::Raw(c) => Some(&c.data),
            ChunkState::Ready(c) => Some(&c.data),
            ChunkState::Processing => None,
        }
    }
}

/// Unified error type for the hfm-reader crate.
#[derive(Debug, thiserror::Error)]
pub enum ReaderError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Offset {0:?} is out of bounds (document length: {1:?})")]
    OffsetOutOfBounds(Offset, Offset),

    #[error("Invalid UTF-8 at offset {0:?}: {1}")]
    InvalidUtf8(Offset, String),

    #[error("PDF parsing failed: {0}")]
    PdfParse(String),

    #[error("Filter error: {0}")]
    Filter(String),

    #[error("Seek error: {0}")]
    Seek(String),

    #[error("Buffer full: cannot insert chunk at offset {0:?}")]
    BufferFull(Offset),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type using `ReaderError`.
pub type ReaderResult<T> = Result<T, ReaderError>;