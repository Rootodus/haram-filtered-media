//! Mock text filters for testing and development.

use crate::filter::TextFilter;
use crate::types::{ProcessedChunk, RawChunk, ReaderResult};

/// A filter that converts the raw text to uppercase.
/// This is useful for testing the pipeline logic without ONNX.
#[derive(Debug, Default)]
pub struct UppercaseFilter;

impl UppercaseFilter {
    pub fn new() -> Self {
        Self
    }
}

impl TextFilter for UppercaseFilter {
    fn process(&self, raw: &RawChunk) -> ReaderResult<ProcessedChunk> {
        let transformed = raw.data.to_uppercase();
        Ok(ProcessedChunk::new(raw.offset, transformed, raw.generation))
    }
}

/// A filter that returns the raw chunk unchanged (no-op).
/// Useful for testing buffer behavior without any transformation.
#[derive(Debug, Default)]
pub struct NoopFilter;

impl NoopFilter {
    pub fn new() -> Self {
        Self
    }
}

impl TextFilter for NoopFilter {
    fn process(&self, raw: &RawChunk) -> ReaderResult<ProcessedChunk> {
        Ok(ProcessedChunk::new(raw.offset, raw.data.clone(), raw.generation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Offset;

    #[test]
    fn test_uppercase() {
        let raw = RawChunk::new(Offset(0), "hello world", 0);
        let filter = UppercaseFilter;
        let processed = filter.process(&raw).unwrap();
        assert_eq!(processed.data, "HELLO WORLD");
        assert_eq!(processed.offset, Offset(0));
    }

    #[test]
    fn test_noop() {
        let raw = RawChunk::new(Offset(0), "keep as is", 0);
        let filter = NoopFilter;
        let processed = filter.process(&raw).unwrap();
        assert_eq!(processed.data, "keep as is");
        assert_eq!(processed.offset, Offset(0));
    }
}