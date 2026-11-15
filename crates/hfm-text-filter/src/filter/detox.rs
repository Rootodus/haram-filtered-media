//! Orchestration of the BART model, tying together ONNX, tokenization, and masking.

use crate::filter::TextFilter;
use crate::mask::TokenMask;
use crate::onnx::BartOnnxModel;
use crate::tokenizer::BartTokenizer;
use crate::types::{ProcessedChunk, RawChunk, ReaderError, ReaderResult};
use std::path::Path;

pub struct BartDetoxFilter {
    model: BartOnnxModel,
    tokenizer: BartTokenizer,
    mask: TokenMask,
    max_new_tokens: usize,
    eos_token_id: i64,
}

impl BartDetoxFilter {
    pub fn new(
        model_dir: &Path,
        banned_words: &[String],
        max_new_tokens: usize,
        eos_token_id: i64,
    ) -> Result<Self, ReaderError> {
        todo!("Construct BartOnnxModel, BartTokenizer, and TokenMask")
    }

    fn generate(&self, input_text: &str) -> ReaderResult<String> {
        todo!("The Greedy Decoding loop goes here")
    }
}

impl TextFilter for BartDetoxFilter {
    fn process(&self, raw: &RawChunk) -> ReaderResult<ProcessedChunk> {
        let output_text = self.generate(&raw.data)?;
        Ok(ProcessedChunk::new(output_text))
    }
}