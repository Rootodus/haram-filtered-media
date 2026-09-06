#![allow(unused)] // temporary while building

pub mod buffer;
pub mod source;
pub mod filter;
pub mod pipeline;

// Re-export key traits and types (to be filled after implementations)
// pub use buffer::TextBuffer;
// pub use source::TextSource;
// pub use filter::TextFilter;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert!(true);
    }
}