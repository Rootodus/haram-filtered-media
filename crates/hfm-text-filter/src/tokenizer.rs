//! Wrapper around the HuggingFace tokenizer to isolate it from ONNX logic.

use crate::types::tensor::TokenIds;
use crate::types::{ReaderError, ReaderResult};
use std::path::Path;
use tokenizers::Tokenizer;

const MAX_TOKENS: usize = 1024;

pub struct BartTokenizer {
    inner: Tokenizer,
}

impl BartTokenizer {
    pub fn new(model_dir: &Path) -> Result<Self, ReaderError> {
        let path = model_dir.join("tokenizer.json");
        let inner = Tokenizer::from_file(&path).map_err(|e| {
            ReaderError::Tokenizer(format!("Failed to load tokenizer from {:?}: {}", path, e))
        })?;
        Ok(Self { inner })
    }

    /// Encode a string into token IDs.
    /// Adds BOS/EOS tokens and truncates to MAX_TOKENS to prevent ONNX shape panics.
    pub fn encode(&self, text: &str) -> ReaderResult<TokenIds> {
        let mut encoding = self.inner.encode(text, true).map_err(|e| {
            ReaderError::Tokenizer(format!("Failed to encode text: {}", e))
        })?;
        
        // Ensure the sequence doesn't exceed BART's maximum length.
        encoding.truncate(MAX_TOKENS, 0, tokenizers::TruncationDirection::Right);
        
        let ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        Ok(TokenIds(ids))
    }

    /// Decode token IDs back to a string.
    /// Skips special tokens during decoding.
    pub fn decode(&self, ids: &TokenIds) -> ReaderResult<String> {
        let u32_ids: Vec<u32> = ids.0.iter().map(|&x| x as u32).collect();
        self.inner
            .decode(&u32_ids, true)
            .map_err(|e| ReaderError::Tokenizer(format!("Failed to decode tokens: {}", e)))
    }

    /// Returns the size of the vocabulary.
    pub fn vocab_size(&self) -> usize {
        self.inner.get_vocab_size(true)
    }
}