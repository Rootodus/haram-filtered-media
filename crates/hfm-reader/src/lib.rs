#![allow(unused)] // temporary while building

pub mod buffer;
pub mod source;
pub mod filter;
pub mod pipeline;
pub mod types;

// Re-export key types and traits
pub use types::{Offset, RawChunk, ProcessedChunk, ChunkState, ReaderError};
// Re-export traits and implementations
pub use buffer::{TextBuffer, TextBufferImpl};
pub use source::{TextSource, PullOutcome, LocalFileSource};
pub use filter::{TextFilter, UppercaseFilter, NoopFilter};
pub use pipeline::{PipelineController, PipelineCommand, PipelineState};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert!(true);
    }
}