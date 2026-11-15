//! Orchestration of the BART model, tying together ONNX, tokenization, and masking.

use crate::cache::PopulatedCache;
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
    banned_words: Vec<String>,
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

        Ok(Self {
            model,
            tokenizer,
            mask,
            banned_words,
            max_new_tokens,
            eos_token_id,
            decoder_start_token_id,
        })
    }

    /// The safety net. Replaces any banned word that slipped past the mask.
    fn apply_fallback_regex(&self, text: &str) -> String {
        let mut result = text.to_string();
        let lower = result.to_lowercase();

        for word in &self.banned_words {
            let word_lower = word.to_lowercase();
            if lower.contains(&word_lower) {
                // Very basic replacement. In the future, we can use a RegexSet for boundary checks.
                // For now, we just replace the substring case-insensitively.
                let re = regex::Regex::new(&format!("(?i){}", regex::escape(word))).unwrap();
                result = re.replace_all(&result, "[redacted]").to_string();
            }
        }
        result
    }

    fn generate(&mut self, input_text: &str) -> ReaderResult<String> {
        // 1. Preprocessing
        let input_ids = self.tokenizer.encode(input_text)?;
        let seq_len = input_ids.0.len();
        
        // Since we process one string at a time, the attention mask is all 1s.
        let attention_mask = AttentionMask(vec![1; seq_len]);
        
        // 2. Run Encoder
        let encoder_hidden = self.model.run_encoder(&input_ids, &attention_mask)?;

        // 3. Generation Loop
        let mut generated_tokens = vec![self.decoder_start_token_id];
        let mut current_input = TokenIds(vec![self.decoder_start_token_id]);
        let mut cache: Option<PopulatedCache> = None;
        
        for step in 0..self.max_new_tokens {
            // Run decoder step (one or with past)
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
            
            // Check for EOS
            if next_token == self.eos_token_id {
                break;
            }

            // Setup input for the next step (just the single last token)
            current_input = TokenIds(vec![next_token]);
        }

        // 4. Postprocessing
        // Exclude the initial decoder_start_token_id when decoding the final string.
        let output_tokens = TokenIds(generated_tokens[1..].to_vec());
        let decoded = self.tokenizer.decode(&output_tokens)?;

        // 5. Fallback
        let cleaned = self.apply_fallback_regex(&decoded);

        Ok(cleaned)
    }
}

impl TextFilter for BartDetoxFilter {
    fn process(&self, raw: &RawChunk) -> ReaderResult<ProcessedChunk> {
        // Note: `generate` needs `&mut self` because `Session::run` mutates the ONNX session.
        // We'll need to change the trait signature or wrap `self` in a Mutex if we share it across threads.
        // For now, let's assume we will wrap the filter in an Arc<Mutex<>> on the server side.
        todo!("We need to handle mutability. See discussion below.")
    }
}