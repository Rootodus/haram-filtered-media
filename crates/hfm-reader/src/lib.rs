#![allow(unused)] // temporary while building

pub mod buffer;
pub mod source;
pub mod filter;
pub mod pipeline;
pub mod types;

// Re-export key types and traits
pub use types::{Offset, RawChunk, ProcessedChunk, ChunkState, ReaderError};
// Re-export traits
pub use buffer::TextBuffer;
pub use source::{TextSource, PullOutcome, LocalFileSource};
pub use filter::TextFilter;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert!(true);
    }
}