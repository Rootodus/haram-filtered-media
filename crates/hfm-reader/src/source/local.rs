//! Local file source: reads a plain text file line by line.

use crate::source::PullOutcome;
use crate::source::TextSource;
use crate::types::{Offset, RawChunk, ReaderError, ReaderResult};
use std::fs;
use std::path::Path;
use std::time::Duration;

/// A source that reads a local plain text file.
///
/// The file is read entirely into memory at construction. Chunks are split at
/// newline boundaries (`\n`), with each line returned as a separate raw chunk.
/// The source supports seeking to arbitrary byte offsets.
pub struct LocalFileSource {
    /// Full content of the file.
    content: String,
    /// Start offsets of each line (in bytes).
    line_offsets: Vec<Offset>,
    /// Index of the next line to return.
    current_idx: usize,
}

impl LocalFileSource {
    /// Create a new `LocalFileSource` from a file path.
    ///
    /// # Errors
    /// Returns `ReaderError::Io` if the file cannot be read.
    pub fn new(path: impl AsRef<Path>) -> ReaderResult<Self> {
        let content = fs::read_to_string(path.as_ref()).map_err(|e| ReaderError::Io(e))?;

        // Compute line start offsets.
        let mut line_offsets = Vec::new();
        line_offsets.push(Offset::ZERO);
        for (i, ch) in content.char_indices() {
            if ch == '\n' {
                let next_offset = Offset((i + 1) as u64);
                // Avoid duplicates if consecutive newlines.
                if let Some(last) = line_offsets.last() {
                    if *last != next_offset {
                        line_offsets.push(next_offset);
                    }
                }
            }
        }
        // Ensure the last line is represented.
        // If the file is non-empty and doesn't end with \n, the final offset
        // is content.len().
        if !content.is_empty() {
            let last = *line_offsets.last().unwrap_or(&Offset::ZERO);
            let content_len = Offset(content.len() as u64);
            if last != content_len {
                line_offsets.push(content_len);
            }
        }

        Ok(Self {
            content,
            line_offsets,
            current_idx: 0,
        })
    }

    /// Returns the total number of lines.
    pub fn line_count(&self) -> usize {
        self.line_offsets.len()
    }

    /// Returns the full content as a string slice.
    pub fn content(&self) -> &str {
        &self.content
    }
}

impl TextSource for LocalFileSource {
    fn try_pull_chunk(&mut self, _timeout: Duration) -> PullOutcome {
        if self.current_idx >= self.line_offsets.len() {
            return PullOutcome::Eos;
        }

        let start = self.line_offsets[self.current_idx];
        let end = if self.current_idx + 1 < self.line_offsets.len() {
            self.line_offsets[self.current_idx + 1]
        } else {
            Offset(self.content.len() as u64)
        };

        let start_usize = start.as_usize();
        let end_usize = end.as_usize();
        let data = self.content[start_usize..end_usize].to_string();
        self.current_idx += 1;

        PullOutcome::Chunk(RawChunk::new(start, data))
    }

    fn seek(&mut self, offset: Offset) -> ReaderResult<()> {
        let target = offset.as_u64();
        // Binary search for the first line offset >= target.
        let idx = self
            .line_offsets
            .binary_search_by_key(&target, |o| o.as_u64())
            .unwrap_or_else(|i| i);
        self.current_idx = idx;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_local_source_basic() {
        let source = LocalFileSource::new("Cargo.toml").unwrap();
        assert!(source.line_count() > 0);
        assert!(!source.content().is_empty());
    }

    #[test]
    fn test_pull_and_seek() {
        let content = "line1\nline2\nline3";
        use std::io::Write;
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path();
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let mut source = LocalFileSource::new(path).unwrap();
        assert_eq!(source.line_count(), 3);

        // Pull first line.
        match source.try_pull_chunk(Duration::from_secs(0)) {
            PullOutcome::Chunk(chunk) => {
                assert_eq!(chunk.offset, Offset::ZERO);
                assert_eq!(chunk.data, "line1");
                // generation is set by the pipeline, not the source, so it should be 0 by default.
                // In tests, the source doesn't set it; it will be set by the pipeline.
                // We'll skip checking generation here because it's not set.
            }
            _ => panic!("Expected chunk"),
        }

        // Pull second line.
        match source.try_pull_chunk(Duration::from_secs(0)) {
            PullOutcome::Chunk(chunk) => {
                assert_eq!(chunk.offset, Offset(5));
                assert_eq!(chunk.data, "line2");
            }
            _ => panic!("Expected chunk"),
        }

        // Seek back to offset 0.
        source.seek(Offset::ZERO).unwrap();
        match source.try_pull_chunk(Duration::from_secs(0)) {
            PullOutcome::Chunk(chunk) => {
                assert_eq!(chunk.offset, Offset::ZERO);
                assert_eq!(chunk.data, "line1");
            }
            _ => panic!("Expected chunk"),
        }

        // Seek to offset 6 (start of "line2").
        source.seek(Offset(6)).unwrap();
        match source.try_pull_chunk(Duration::from_secs(0)) {
            PullOutcome::Chunk(chunk) => {
                assert_eq!(chunk.offset, Offset(5));
                assert_eq!(chunk.data, "line2");
            }
            _ => panic!("Expected chunk"),
        }

        // Seek to end of file.
        source.seek(Offset(content.len() as u64)).unwrap();
        match source.try_pull_chunk(Duration::from_secs(0)) {
            PullOutcome::Eos => {}
            _ => panic!("Expected Eos"),
        }
    }
}
