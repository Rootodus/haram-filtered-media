//! Orchestration of the BART model, tying together ONNX, tokenization, and masking.

use crate::cache::PopulatedCache;
use crate::fallback_regex::FallbackRegex;
use crate::filter::TextFilter;
use crate::mask::TokenMask;
use crate::onnx::BartOnnxModel;
use crate::tokenizer::BartTokenizer;
use crate::types::tensor::{AttentionMask, TokenIds};
use crate::types::{ProcessedChunk, RawChunk, ReaderError, ReaderResult};
use std::path::Path;

pub struct BartDetoxFilter {
    model: BartOnnxModel,
    tokenizer: BartTokenizer,
    mask: TokenMask,
    fallback_regex: FallbackRegex,
    max_new_tokens: usize,
    eos_token_id: i64,
    decoder_start_token_id: i64,
}

impl BartDetoxFilter {
    pub fn new(
        model_dir: &Path,
        banned_words: Vec<String>,
        max_new_tokens: usize,
        eos_token_id: i64,
        decoder_start_token_id: i64,
    ) -> Result<Self, ReaderError> {
        let model = BartOnnxModel::new(model_dir)?;
        let tokenizer = BartTokenizer::new(model_dir)?;
        let mask = TokenMask::new(&banned_words, &tokenizer);
        let fallback_regex = FallbackRegex::new(&banned_words);

        Ok(Self {
            model,
            tokenizer,
            mask,
            fallback_regex,
            max_new_tokens,
            eos_token_id,
            decoder_start_token_id,
        })
    }

    /// The safety net. Replaces any banned word that slipped past the mask.
    fn apply_fallback_regex(&self, text: &str) -> String {
        self.fallback_regex.apply(text)
    }

    fn generate(&mut self, input_text: &str) -> ReaderResult<String> {
        // 1. Preprocessing
        let input_ids = self.tokenizer.encode(input_text)?;
        let seq_len = input_ids.0.len();
        let attention_mask = AttentionMask(vec![1; seq_len]);
        
        // 2. Run Encoder
        let encoder_hidden = self.model.run_encoder(&input_ids, &attention_mask)?;

        // 3. Generation Loop
        let mut generated_tokens = vec![self.decoder_start_token_id];
        let mut current_input = TokenIds(vec![self.decoder_start_token_id]);
        let mut cache: Option<PopulatedCache> = None;
        
        for _step in 0..self.max_new_tokens {
            let (mut logits, new_cache) = if let Some(past) = cache.take() {
                self.model.run_decoder_step_with_past(&current_input, &encoder_hidden, &attention_mask, past)?
            } else {
                self.model.run_decoder_step_one(&current_input, &encoder_hidden, &attention_mask)?
            };
            
            cache = Some(new_cache);

            // Apply Trie mask
            self.mask.apply_mask(&mut logits, &generated_tokens);

            // Select next token
            let next_token = TokenMask::select_next_token(&logits)?;
            
            generated_tokens.push(next_token);
            
            if next_token == self.eos_token_id {
                break;
            }

            current_input = TokenIds(vec![next_token]);
        }

        // 4. Postprocessing
        let output_tokens = TokenIds(generated_tokens[1..].to_vec());
        let decoded = self.tokenizer.decode(&output_tokens)?;

        // 5. Fallback
        let cleaned = self.apply_fallback_regex(&decoded);

        Ok(cleaned)
    }
}

impl TextFilter for BartDetoxFilter {
    fn process(&mut self, raw: &RawChunk) -> ReaderResult<ProcessedChunk> {
        let output_text = self.generate(&raw.data)?;
        Ok(ProcessedChunk::new(output_text))
    }
}