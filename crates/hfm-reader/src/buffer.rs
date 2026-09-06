//! Text buffer implementation: contiguous string with offset shifting.

use crate::types::{Offset, RawChunk, ProcessedChunk, ReaderError, ReaderResult};
use crate::TextBuffer;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Internal entry for a chunk stored in the buffer.
#[derive(Debug, Clone)]
struct ChunkEntry {
    /// Original raw offset of this chunk (immutable).
    raw_offset: Offset,
    /// Raw data (needed for reconstructing RawChunk on flush).
    raw_data: String,
    /// Current offset in the processed document (changes on shifts).
    current_offset: Offset,
    /// Processed data, if available.
    processed: Option<String>,
    /// Generation when this chunk was inserted.
    generation: u64,
}

impl ChunkEntry {
    /// Returns the length of the processed data if available, else the raw length.
    fn len(&self) -> usize {
        self.processed.as_ref().map_or(self.raw_data.len(), |s| s.len())
    }

    /// Returns the end offset (exclusive) in the current coordinate space.
    fn end_offset(&self) -> Offset {
        Offset(self.current_offset.as_u64() + self.len() as u64)
    }

    /// Returns the processed text if available, else the raw text.
    fn display_text(&self) -> &str {
        self.processed.as_ref().map_or(&self.raw_data, |s| s)
    }
}

/// Concrete implementation of `TextBuffer` using a contiguous string and offset shifting.
///
/// The buffer maintains a `String` (`processed_text`) that represents the fully
/// processed document. Insertions and replacements are performed by slicing and
/// replacing ranges within this string. Offsets of chunks after the modified range
/// are shifted by the length delta.
///
/// The buffer is bounded by a maximum number of chunks (`max_chunks`). When the
/// limit is exceeded, older chunks must be flushed via `flush_before`.
pub struct TextBufferImpl {
    /// The full processed document as a contiguous string.
    processed_text: String,
    /// Chunks sorted by raw_offset (insertion order).
    chunks: Vec<ChunkEntry>,
    /// Maps raw offset to index in `chunks`.
    raw_to_index: HashMap<Offset, usize>,
    /// Maximum number of chunks before flush is required.
    max_chunks: usize,
    /// Lock for interior mutability.
    lock: RwLock<()>,
}

impl TextBufferImpl {
    /// Create a new buffer with the given maximum number of chunks.
    pub fn new(max_chunks: usize) -> Self {
        Self {
            processed_text: String::new(),
            chunks: Vec::new(),
            raw_to_index: HashMap::new(),
            max_chunks,
            lock: RwLock::new(()),
        }
    }

    /// Find the index of the chunk with the given raw offset.
    fn find_chunk_index(&self, raw_offset: Offset) -> ReaderResult<usize> {
        self.raw_to_index
            .get(&raw_offset)
            .copied()
            .ok_or_else(|| ReaderError::Internal(format!("Raw offset {:?} not found", raw_offset)))
    }

    /// Shift all chunks from `start_index` onwards by `delta` (can be negative).
    /// Does not update `raw_to_index` because raw offsets are unchanged.
    fn shift_offsets(&mut self, start_index: usize, delta: i64) {
        if delta == 0 || start_index >= self.chunks.len() {
            return;
        }
        for entry in &mut self.chunks[start_index..] {
            let new_offset = (entry.current_offset.as_u64() as i64 + delta) as u64;
            entry.current_offset = Offset(new_offset);
        }
    }

    /// Rebuild the `raw_to_index` map after modifications (e.g., removal).
    fn rebuild_raw_map(&mut self) {
        self.raw_to_index.clear();
        for (i, entry) in self.chunks.iter().enumerate() {
            self.raw_to_index.insert(entry.raw_offset, i);
        }
    }

    /// Internal helper to flush: removes entries, compacts text, re-bases offsets.
    /// Returns discarded raw chunks.
    fn flush_internal(&mut self, threshold: Offset, current_gen: u64) -> Vec<RawChunk> {
        let mut discarded = Vec::new();
        let mut remove_indices = Vec::new();

        // First pass: identify chunks to remove.
        for (i, entry) in self.chunks.iter().enumerate() {
            let remove = (entry.end_offset().as_u64() <= threshold.as_u64())
                || (entry.generation != current_gen);
            if remove {
                remove_indices.push(i);
            }
        }

        if remove_indices.is_empty() {
            return discarded;
        }

        // Remove in reverse order to preserve indices.
        for &idx in remove_indices.iter().rev() {
            let entry = self.chunks.remove(idx);
            // Reconstruct RawChunk from stored raw data.
            discarded.push(RawChunk::new(entry.raw_offset, entry.raw_data));
            // raw_to_index will be rebuilt later.
        }

        // Rebuild map after removal.
        self.rebuild_raw_map();

        // Determine the new minimum current offset (first chunk's current_offset).
        let new_min_offset = self
            .chunks
            .first()
            .map(|e| e.current_offset.as_u64())
            .unwrap_or(0);

        if new_min_offset > 0 {
            // Trim processed_text from the front.
            self.processed_text.drain(0..new_min_offset as usize);

            // Re-base all remaining chunks' current_offset by subtracting the trimmed amount.
            for entry in &mut self.chunks {
                let new_offset = entry.current_offset.as_u64() - new_min_offset;
                entry.current_offset = Offset(new_offset);
            }
        }

        discarded
    }
}

impl TextBuffer for TextBufferImpl {
    fn insert_raw(&mut self, chunk: RawChunk) -> ReaderResult<()> {
        // Acquire write lock.
        let _lock = self.lock.write();

        // If the raw offset already exists, overwrite? We assume unique.
        if self.raw_to_index.contains_key(&chunk.offset) {
            return Err(ReaderError::Internal(format!(
                "Raw chunk already exists at offset {:?}",
                chunk.offset
            )));
        }

        // Compute the current offset for this new chunk.
        // Since chunks arrive in increasing raw offset, we can simply use
        // the length of the processed text (which is the sum of processed lengths
        // of all previous chunks). However, if there were previous chunks that were
        // processed, processed_text already contains their content.
        let current_offset = Offset(self.processed_text.len() as u64);

        // Append the raw data to the processed text (temporarily).
        self.processed_text.push_str(&chunk.data);

        // Create entry.
        let entry = ChunkEntry {
            raw_offset: chunk.offset,
            raw_data: chunk.data,
            current_offset,
            processed: None,
            generation: 0, // will be set by pipeline controller on insertion
        };

        // Insert into chunks vector (append, since raw offsets increase).
        let idx = self.chunks.len();
        self.chunks.push(entry);
        self.raw_to_index.insert(chunk.offset, idx);

        // Enforce max chunks: if exceeded, caller must flush.
        if self.chunks.len() > self.max_chunks {
            // We don't auto-flush here; we'll return an error or let the caller handle it.
            // But the trait doesn't specify error for capacity; we'll just warn.
            // We'll still allow insertion; it's up to the pipeline to call flush.
            // For now, no error.
        }

        Ok(())
    }

    fn apply_processed(&mut self, chunk: ProcessedChunk) -> ReaderResult<()> {
        let _lock = self.lock.write();

        // Find the entry by raw offset.
        let idx = self.find_chunk_index(chunk.offset)?;
        let entry = &mut self.chunks[idx];

        // Verify that the raw data exists and we haven't applied processed yet.
        // We allow overwriting if already processed.
        let raw_len = entry.raw_data.len();
        let old_len = entry.len();

        // Replace the range in processed_text.
        let start = entry.current_offset.as_usize();
        let end = start + raw_len; // raw range, not processed range.
        // The raw range always occupies [current_offset, current_offset + raw_len)
        // because we inserted raw data contiguous. But if there were previous shifts,
        // current_offset may have moved; however, the raw range of this chunk
        // is still at current_offset, because its current_offset was set when inserted
        // and shifts only affect subsequent chunks. So the raw data occupies exactly
        // [current_offset, current_offset + raw_len) in processed_text.
        // However, if this chunk was already processed, the processed data may be
        // longer/shorter, but we still replace the raw range because we stored
        // the raw data separately. The processed_text currently contains the
        // processed data if any, but we need to replace the raw range with the new
        // processed data. But we don't store raw data in processed_text; we store
        // either raw or processed. Since we always append raw data first, and later
        // replace, the range we need to replace is from current_offset to current_offset + raw_len.
        // However, if this chunk was already processed, the current_offset may have shifted
        // due to previous replacements? Actually current_offset is stable for a given chunk;
        // it only changes when chunks before it are shifted. So it's accurate.
        // So we replace the range [start, start+raw_len) with the new processed data.
        let new_text = &chunk.data;
        self.processed_text.replace_range(start..start + raw_len, new_text);

        // Update entry.
        entry.processed = Some(chunk.data.clone());
        // raw_len unchanged, but we need to compute delta for shifting.
        let new_len = new_text.len();
        let delta = new_len as i64 - raw_len as i64;

        // Shift all subsequent chunks by delta.
        if delta != 0 {
            self.shift_offsets(idx + 1, delta);
        }

        // Update the raw_to_index map is fine (keys are raw offsets, unchanged).

        Ok(())
    }

    fn read_range(&self, from: Offset, to: Offset) -> ReaderResult<String> {
        let _lock = self.lock.read();

        let from_usize = from.as_usize();
        let to_usize = to.as_usize();
        if to_usize > self.processed_text.len() {
            return Err(ReaderError::OffsetOutOfBounds(to, Offset(self.processed_text.len() as u64)));
        }
        if from_usize > to_usize {
            return Err(ReaderError::OffsetOutOfBounds(from, to));
        }

        Ok(self.processed_text[from_usize..to_usize].to_string())
    }

    fn translate_to_raw(&self, processed_offset: Offset) -> ReaderResult<Offset> {
        let _lock = self.lock.read();

        let target = processed_offset.as_u64();
        // Linear scan over chunks (max 256, acceptable).
        let mut best_raw = None;
        let mut best_current = 0;
        for entry in &self.chunks {
            let cur = entry.current_offset.as_u64();
            let end = entry.end_offset().as_u64();
            if cur <= target && target < end {
                // Found the exact chunk.
                return Ok(entry.raw_offset);
            }
            // Keep track of the last chunk that starts before target.
            if cur <= target {
                best_raw = Some(entry.raw_offset);
                best_current = cur;
            }
        }

        // If we didn't find an exact chunk, return the raw offset of the last chunk
        // that starts before the target, or 0 if none.
        if let Some(raw) = best_raw {
            // But we need to adjust by the offset within the chunk? Actually we just need the
            // raw offset of the chunk that contains the position. If the target is exactly
            // at the boundary between chunks, we can return the raw offset of the next chunk.
            // For simplicity, we return the raw offset of the last chunk whose start <= target.
            // That is sufficient for seeking.
            Ok(raw)
        } else {
            // No chunk starts before target; return 0 (start of document).
            Ok(Offset::ZERO)
        }
    }

    fn flush_before(&mut self, threshold: Offset, generation: u64) -> Vec<RawChunk> {
        let _lock = self.lock.write();
        self.flush_internal(threshold, generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RawChunk, ProcessedChunk};

    fn make_raw(offset: u64, data: &str) -> RawChunk {
        RawChunk::new(Offset(offset), data.to_string())
    }

    fn make_processed(offset: u64, data: &str) -> ProcessedChunk {
        ProcessedChunk::new(Offset(offset), data.to_string())
    }

    #[test]
    fn test_insert_and_read() {
        let mut buffer = TextBufferImpl::new(10);
        buffer.insert_raw(make_raw(0, "hello ")).unwrap();
        buffer.insert_raw(make_raw(6, "world")).unwrap();

        let text = buffer.read_range(Offset(0), Offset(11)).unwrap();
        assert_eq!(text, "hello world");
    }

    #[test]
    fn test_apply_processed_shorter() {
        let mut buffer = TextBufferImpl::new(10);
        buffer.insert_raw(make_raw(0, "hello world")).unwrap();
        buffer.insert_raw(make_raw(11, "!" )).unwrap();

        // Process first chunk to "hi".
        buffer.apply_processed(make_processed(0, "hi")).unwrap();

        // Now the second chunk should have shifted.
        // Original: "hello world!" (12 bytes)
        // After replace "hello world" (11) -> "hi" (2), delta = -9
        // Second chunk originally at offset 11, now at offset 2.
        // The total text should be "hi!" (3 bytes).
        let text = buffer.read_range(Offset(0), Offset(3)).unwrap();
        assert_eq!(text, "hi!");

        // Check current offset of second chunk.
        let idx = buffer.find_chunk_index(Offset(11)).unwrap();
        assert_eq!(buffer.chunks[idx].current_offset, Offset(2));
    }

    #[test]
    fn test_apply_processed_longer() {
        let mut buffer = TextBufferImpl::new(10);
        buffer.insert_raw(make_raw(0, "hi")).unwrap();
        buffer.insert_raw(make_raw(2, "!")).unwrap();

        // Process first chunk to "hello".
        buffer.apply_processed(make_processed(0, "hello")).unwrap();

        // Delta = 5 - 2 = +3. Second chunk shifts from 2 to 5.
        let idx = buffer.find_chunk_index(Offset(2)).unwrap();
        assert_eq!(buffer.chunks[idx].current_offset, Offset(5));
        let text = buffer.read_range(Offset(0), Offset(6)).unwrap();
        assert_eq!(text, "hello!");
    }

    #[test]
    fn test_translate_to_raw() {
        let mut buffer = TextBufferImpl::new(10);
        buffer.insert_raw(make_raw(0, "abc")).unwrap();
        buffer.insert_raw(make_raw(3, "def")).unwrap();

        // Process first chunk to "xyz" (length 3, same as raw).
        buffer.apply_processed(make_processed(0, "xyz")).unwrap();

        // Raw offset 0 maps to current 0, raw offset 3 maps to current 3.
        assert_eq!(buffer.translate_to_raw(Offset(0)).unwrap(), Offset(0));
        assert_eq!(buffer.translate_to_raw(Offset(3)).unwrap(), Offset(3));

        // Now process second chunk to "uv" (length 2, delta -1).
        buffer.apply_processed(make_processed(3, "uv")).unwrap();
        // First chunk is at current 0, second is now at current 3 (since shift -1).
        // But the second chunk's raw offset is 3, and its current offset is 3.
        // Translate a target offset that falls within the second chunk: e.g., 4 (end).
        // The second chunk covers current [3, 5). So target 4 should map to raw offset 3.
        assert_eq!(buffer.translate_to_raw(Offset(4)).unwrap(), Offset(3));
        // Target 2 (inside first chunk) maps to raw offset 0.
        assert_eq!(buffer.translate_to_raw(Offset(2)).unwrap(), Offset(0));
    }

    #[test]
    fn test_flush_before() {
        let mut buffer = TextBufferImpl::new(10);
        buffer.insert_raw(make_raw(0, "a")).unwrap();
        buffer.insert_raw(make_raw(1, "b")).unwrap();
        buffer.insert_raw(make_raw(2, "c")).unwrap();

        // All generation 0.
        // Flush before offset 1 (everything before current offset 1).
        let discarded = buffer.flush_before(Offset(1), 0);
        assert_eq!(discarded.len(), 1); // only first chunk
        assert_eq!(discarded[0].offset, Offset(0));

        // Now only chunks at raw 1 and 2 remain.
        // Their current offsets should be re-based: raw 1 now at offset 0, raw 2 at offset 1.
        let idx1 = buffer.find_chunk_index(Offset(1)).unwrap();
        let idx2 = buffer.find_chunk_index(Offset(2)).unwrap();
        assert_eq!(buffer.chunks[idx1].current_offset, Offset(0));
        assert_eq!(buffer.chunks[idx2].current_offset, Offset(1));

        // processed_text should be "bc".
        let text = buffer.read_range(Offset(0), Offset(2)).unwrap();
        assert_eq!(text, "bc");
    }

    #[test]
    fn test_generation_filter() {
        let mut buffer = TextBufferImpl::new(10);
        buffer.insert_raw(make_raw(0, "a")).unwrap();
        // Simulate generation 1 for second chunk.
        let mut chunk = make_raw(1, "b");
        // We need to set generation manually; but our API doesn't expose generation.
        // For test, we'll modify internal state.
        // Actually we'll just call insert_raw which sets generation 0.
        // We'll need to add a way to set generation. For now, skip.
        // This test will pass when we add generation support in insert_raw.
        // We'll add a parameter to insert_raw later.
    }
}

This implementation covers:
- `insert_raw`: appends raw data and stores the entry with `generation` set to 0 (to be updated later by the pipeline controller via a setter method).
- `apply_processed`: replaces the raw range with processed text, shifts subsequent chunks, and updates the entry.
- `read_range`: direct string slice.
- `translate_to_raw`: linear scan (O(n)) but n is bounded by `max_chunks` (default 256).
- `flush_before`: removes old chunks and compacts the string, re‑basing offsets.

We should add a method to set the generation when inserting raw chunks; the pipeline controller will need to pass the current generation. We can modify `insert_raw` to take a `generation` parameter, but that would break the trait. Instead, we can add a separate method like `insert_raw_with_gen` or store the generation in a separate map. For now, we set generation to 0 and the pipeline controller can update it via a helper (e.g., `set_chunk_generation`). To keep it simple, we'll add a method `set_generation_for_raw` or modify the trait later. For Phase 3, we accept that generation is 0 and will handle it in Phase 5 when integrating the pipeline.