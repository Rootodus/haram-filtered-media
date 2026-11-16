//! Mock text filters for testing and development.

use crate::filter::TextFilter;
use crate::types::{ProcessedChunk, RawChunk, FilterResult};

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
    fn process(&mut self, raw: &RawChunk) -> FilterResult<ProcessedChunk> {
        let transformed = raw.data.to_uppercase();
        Ok(ProcessedChunk::new(transformed))
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
    fn process(&mut self, raw: &RawChunk) -> FilterResult<ProcessedChunk> {
        Ok(ProcessedChunk::new(raw.data.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uppercase() {
        let raw = RawChunk::new("hello world");
        let mut filter = UppercaseFilter;
        let processed = filter.process(&raw).unwrap();
        assert_eq!(processed.data, "HELLO WORLD");
    }

    #[test]
    fn test_noop() {
        let raw = RawChunk::new("keep as is");
        let mut filter = NoopFilter;
        let processed = filter.process(&raw).unwrap();
        assert_eq!(processed.data, "keep as is");
    }
}