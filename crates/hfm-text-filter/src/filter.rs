//! Text filter abstraction and implementations.

pub mod mock;
pub mod detox;

use crate::types::{ProcessedChunk, RawChunk, FilterResult};

/// A filter that processes raw text chunks.
pub trait TextFilter: Send + Sync {
    /// Process a raw chunk and produce a processed chunk.
    /// Takes &mut self because ONNX sessions mutate internal memory during inference.
    fn process(&mut self, raw: &RawChunk) -> FilterResult<ProcessedChunk>;
}

pub use mock::{UppercaseFilter, NoopFilter};
pub use detox::BartDetoxFilter;