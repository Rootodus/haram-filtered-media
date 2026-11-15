//! Wrapper around the HuggingFace tokenizer to isolate it from ONNX logic.

use crate::types::{ReaderError, ReaderResult};
use crate::types::tensor::TokenIds;
use std::path::Path;

pub struct BartTokenizer {
    inner: tokenizers::Tokenizer,
}

impl BartTokenizer {
    pub fn new(model_dir: &Path) -> Result<Self, ReaderError> {
        todo!("Load tokenizer.json from model_dir")
    }

    pub fn encode(&self, text: &str) -> ReaderResult<TokenIds> {
        todo!("Encode text into TokenIds, ensuring special tokens are added")
    }

    pub fn decode(&self, ids: &[i64]) -> String {
        todo!("Decode token IDs back to a string")
    }

    pub fn vocab_size(&self) -> usize {
        todo!("Return the tokenizer vocabulary size")
    }
}