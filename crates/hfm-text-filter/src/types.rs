//! Core data types and errors for the text processing pipeline.

/// A raw (unprocessed) text chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct RawChunk {
    pub data: String,
}

impl RawChunk {
    pub fn new(data: impl Into<String>) -> Self {
        Self { data: data.into() }
    }
}

/// A processed text chunk ready for display.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedChunk {
    pub data: String,
}

impl ProcessedChunk {
    pub fn new(data: impl Into<String>) -> Self {
        Self { data: data.into() }
    }
}

/// Unified error type for the hfm-text-filter crate.
#[derive(Debug, thiserror::Error)]
pub enum ReaderError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Filter error: {0}")]
    Filter(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type using `ReaderError`.
pub type ReaderResult<T> = Result<T, ReaderError>;
