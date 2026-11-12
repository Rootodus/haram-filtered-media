//! BART-based detoxification filter.
//! Uses the `s-nlp/bart-base-detox` model exported to ONNX.

use crate::filter::TextFilter;
use crate::types::{ProcessedChunk, RawChunk, ReaderResult};
use anyhow::{anyhow, Result};
use ort::{session::Session, value::Value};
use parking_lot::Mutex;
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;

/// Splits text into segments that are ≤ max_tokens (as measured by the tokenizer).
/// Tries to preserve sentence boundaries.
fn split_into_segments(text: &str, tokenizer: &Tokenizer, max_tokens: usize) -> Vec<String> {
    // 1. Split on high-priority delimiters: . ! ? \n
    let high_delimiters = ['.', '!', '?', '\n'];
    let mut segments = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        current.push(ch);
        if high_delimiters.contains(&ch) {
            let is_abbrev = ch == '.' && i + 1 < chars.len() && chars[i + 1].is_alphabetic() && chars[i + 1].is_lowercase();
            if !is_abbrev {
                segments.push(current.clone());
                current.clear();
            }
        }
        i += 1;
    }
    if !current.is_empty() {
        segments.push(current);
    }

    // 2. For each segment, check token count; if > max_tokens, split further on ; : , -.
    let mut final_segments = Vec::new();
    for seg in segments {
        let tokens = tokenizer.encode(seg.clone(), true).unwrap();
        if tokens.len() <= max_tokens {
            final_segments.push(seg);
            continue;
        }
        let mut sub_segments = vec![seg];
        for delimiter in [';', ':', ',', '-'] {
            let mut new_sub = Vec::new();
            for sub in sub_segments {
                let parts: Vec<&str> = sub.split(delimiter).collect();
                if parts.len() > 1 {
                    let mut combined = String::new();
                    for (idx, part) in parts.iter().enumerate() {
                        if idx > 0 {
                            combined.push(delimiter);
                        }
                        combined.push_str(part);
                        let tokens = tokenizer.encode(combined.clone(), true).unwrap();
                        if tokens.len() > max_tokens {
                            // split further; we'll fallback to word chunking later
                        }
                    }
                }
            }
            sub_segments = new_sub;
        }
        // As final fallback, split by token count.
        let words: Vec<&str> = seg.split_whitespace().collect();
        let mut current_chunk = String::new();
        for word in words {
            let test = if current_chunk.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current_chunk, word)
            };
            if tokenizer.encode(test.clone(), true).unwrap().len() > max_tokens {
                if !current_chunk.is_empty() {
                    final_segments.push(current_chunk.trim().to_string());
                    current_chunk.clear();
                }
                current_chunk.push_str(word);
                current_chunk.push(' ');
            } else {
                current_chunk.push_str(word);
                current_chunk.push(' ');
            }
        }
        if !current_chunk.is_empty() {
            final_segments.push(current_chunk.trim().to_string());
        }
    }
    final_segments
}

/// BART detoxification filter.
pub struct BartDetoxFilter {
    encoder: Arc<Mutex<Session>>,
    decoder: Arc<Mutex<Session>>,
    tokenizer: Tokenizer,
    max_input_tokens: usize,
    max_output_tokens: usize,
    bos_token_id: i64,
    eos_token_id: i64,
    pad_token_id: i64,
    decoder_start_token_id: i64,
}

impl BartDetoxFilter {
    /// Create a new filter by loading the ONNX model and tokenizer from `path`.
    pub fn new(model_path: &Path, max_input_tokens: usize, max_output_tokens: usize) -> Result<Self> {
        let encoder_path = model_path.join("encoder_model.onnx");
        let decoder_path = model_path.join("decoder_model_merged.onnx");
        let tokenizer_path = model_path.join("tokenizer.json");

        if !encoder_path.exists() || !decoder_path.exists() || !tokenizer_path.exists() {
            return Err(anyhow!("Missing ONNX or tokenizer files in {:?}", model_path));
        }

        let encoder = Session::builder()?
            .with_execution_providers([ort::ep::CPU::default().build()])?
            .commit_from_file(&encoder_path)?;

        let decoder = Session::builder()?
            .with_execution_providers([ort::ep::CPU::default().build()])?
            .commit_from_file(&decoder_path)?;

        let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| anyhow!("Failed to load tokenizer: {}", e))?;

        // Read special token IDs from config.json (or use defaults)
        let config_path = model_path.join("config.json");
        let config: serde_json::Value = if config_path.exists() {
            let content = std::fs::read_to_string(config_path)?;
            serde_json::from_str(&content)?
        } else {
            serde_json::json!({})
        };
        let bos_token_id = config["bos_token_id"].as_i64().unwrap_or(0);
        let eos_token_id = config["eos_token_id"].as_i64().unwrap_or(2);
        let pad_token_id = config["pad_token_id"].as_i64().unwrap_or(1);
        let decoder_start_token_id = config["decoder_start_token_id"].as_i64().unwrap_or(2);

        Ok(Self {
            encoder: Arc::new(Mutex::new(encoder)),
            decoder: Arc::new(Mutex::new(decoder)),
            tokenizer,
            max_input_tokens,
            max_output_tokens,
            bos_token_id,
            eos_token_id,
            pad_token_id,
            decoder_start_token_id,
        })
    }

    /// Run inference on a single segment.
    fn process_segment(&self, text: &str) -> Result<String> {
        let encoder = self.encoder.lock();
        let decoder = self.decoder.lock();

        // Tokenize input
        let encoding = self.tokenizer.encode(text, true).map_err(|e| anyhow!("Tokenization error: {}", e))?;
        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&x| x as i64).collect();

        if input_ids.is_empty() {
            return Ok(text.to_string());
        }

        // Run encoder
        let encoder_inputs = ort::inputs![
            "input_ids" => input_ids.clone().into_dyn().into(),
            "attention_mask" => attention_mask.clone().into_dyn().into(),
        ];
        let encoder_outputs = encoder.run(encoder_inputs)?;
        let encoder_hidden: ort::value::Value = encoder_outputs["last_hidden_state"].to_owned();

        // Prepare decoder initial input
        let mut generated_tokens: Vec<i64> = vec![self.decoder_start_token_id];
        let mut past_tensors: Vec<Value> = Vec::new();
        let max_new_tokens = self.max_output_tokens;

        for step in 0..max_new_tokens {
            let use_cache = !past_tensors.is_empty();
            let current_input_ids = if step == 0 {
                vec![self.decoder_start_token_id]
            } else {
                vec![generated_tokens[generated_tokens.len()-1]]
            };

            let mut decoder_inputs = ort::inputs![
                "input_ids" => current_input_ids.clone().into_dyn().into(),
                "encoder_hidden_states" => encoder_hidden.clone(),
                "encoder_attention_mask" => attention_mask.clone().into_dyn().into(),
                "use_cache_branch" => use_cache.into_dyn().into(),
            ];

            if use_cache {
                let mut idx = 0;
                for layer in 0..6 {
                    decoder_inputs = decoder_inputs.with_name(
                        format!("past_key_values.{}.decoder.key", layer).as_str(),
                        past_tensors[idx].clone(),
                    );
                    idx += 1;
                    decoder_inputs = decoder_inputs.with_name(
                        format!("past_key_values.{}.decoder.value", layer).as_str(),
                        past_tensors[idx].clone(),
                    );
                    idx += 1;
                    decoder_inputs = decoder_inputs.with_name(
                        format!("past_key_values.{}.encoder.key", layer).as_str(),
                        past_tensors[idx].clone(),
                    );
                    idx += 1;
                    decoder_inputs = decoder_inputs.with_name(
                        format!("past_key_values.{}.encoder.value", layer).as_str(),
                        past_tensors[idx].clone(),
                    );
                    idx += 1;
                }
            }

            let decoder_outputs = decoder.run(decoder_inputs)?;
            let logits_tensor = decoder_outputs["logits"].to_owned();
            let logits_data = logits_tensor.try_extract_tensor::<f32>()?;
            let last_logit = logits_data.view();
            let vocab_size = last_logit.shape()[2];
            let next_token = (0..vocab_size)
                .max_by(|&a, &b| last_logit[[0, 0, a]].partial_cmp(&last_logit[[0, 0, b]]).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0) as i64;

            generated_tokens.push(next_token);

            let mut new_past = Vec::new();
            for layer in 0..6 {
                let key_present = decoder_outputs[format!("present.{}.decoder.key", layer).as_str()].to_owned();
                let value_present = decoder_outputs[format!("present.{}.decoder.value", layer).as_str()].to_owned();
                let enc_key_present = decoder_outputs[format!("present.{}.encoder.key", layer).as_str()].to_owned();
                let enc_value_present = decoder_outputs[format!("present.{}.encoder.value", layer).as_str()].to_owned();
                new_past.push(key_present);
                new_past.push(value_present);
                new_past.push(enc_key_present);
                new_past.push(enc_value_present);
            }
            past_tensors = new_past;

            if next_token == self.eos_token_id {
                break;
            }
        }

        let output_tokens: Vec<u32> = generated_tokens[1..].iter().map(|&x| x as u32).collect();
        let decoded = self.tokenizer.decode(&output_tokens, true).map_err(|e| anyhow!("Decode error: {}", e))?;
        Ok(decoded)
    }
}

impl TextFilter for BartDetoxFilter {
    fn process(&self, raw: &RawChunk) -> ReaderResult<ProcessedChunk> {
        let segments = split_into_segments(&raw.data, &self.tokenizer, self.max_input_tokens);
        let mut processed_segments = Vec::new();
        for seg in segments {
            match self.process_segment(&seg) {
                Ok(out) => processed_segments.push(out),
                Err(e) => {
                    eprintln!("Detox filter error: {}", e);
                    processed_segments.push(seg);
                }
            }
        }
        let result = processed_segments.join(" ");
        Ok(ProcessedChunk::new(raw.offset, result, raw.generation))
    }
}