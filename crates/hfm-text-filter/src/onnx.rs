//! Raw ONNX session execution, isolated from tokenization and masking.

use crate::cache::PopulatedCache;
use crate::types::tensor::{AttentionMask, TokenIds};
use crate::types::{ReaderError, ReaderResult};
use ort::session::Session;
use ort::value::Value;
use std::path::Path;

pub struct BartOnnxModel {
    encoder: Session,
    decoder: Session,
    decoder_with_past: Session,
}

impl BartOnnxModel {
    pub fn new(model_dir: &Path) -> Result<Self, ReaderError> {
        let encoder_path = model_dir.join("encoder_model.onnx");
        let decoder_path = model_dir.join("decoder_model.onnx");
        let decoder_past_path = model_dir.join("decoder_with_past_model.onnx");

        let build_session = |path: &Path| -> Result<Session, ReaderError> {
            Session::builder()
                .map_err(|e| ReaderError::Onnx(format!("Builder error: {}", e)))?
                .with_execution_providers([ort::ep::CPU::default().build()])
                .map_err(|e| ReaderError::Onnx(format!("Execution provider error: {}", e)))?
                .commit_from_file(path)
                .map_err(|e| ReaderError::Onnx(format!("Failed to load {:?}: {}", path, e)))
        };

        Ok(Self {
            encoder: build_session(&encoder_path)?,
            decoder: build_session(&decoder_path)?,
            decoder_with_past: build_session(&decoder_past_path)?,
        })
    }

    pub fn run_encoder(&self, ids: &TokenIds, mask: &AttentionMask) -> Result<Value, ReaderError> {
        let seq_len = ids.0.len();
        
        // Create tensors with shape [1, seq_len]
        let input_ids = ndarray::Array2::from_shape_vec((1, seq_len), ids.0.clone())
            .map_err(|e| ReaderError::Internal(format!("Shape error: {}", e)))?;
        let attention_mask = ndarray::Array2::from_shape_vec((1, seq_len), mask.0.clone())
            .map_err(|e| ReaderError::Internal(format!("Shape error: {}", e)))?;

        let inputs = ort::inputs![
            "input_ids" => Value::from_array(input_ids).map_err(|e| ReaderError::Onnx(e.to_string()))?,
            "attention_mask" => Value::from_array(attention_mask).map_err(|e| ReaderError::Onnx(e.to_string()))?,
        ];

        let mut outputs = self.encoder.run(inputs)
            .map_err(|e| ReaderError::Onnx(format!("Encoder run failed: {}", e)))?;

        let hidden = outputs.remove("last_hidden_state")
            .ok_or_else(|| ReaderError::Onnx("Missing last_hidden_state".into()))?;

        Ok(hidden)
    }

    pub fn run_decoder_step_one(
        &self,
        decoder_input_ids: &TokenIds,
        encoder_hidden: &Value,
        attention_mask: &AttentionMask,
    ) -> Result<(Vec<f32>, PopulatedCache), ReaderError> {
        let seq_len = decoder_input_ids.0.len();
        let enc_seq_len = attention_mask.0.len();

        let input_ids = ndarray::Array2::from_shape_vec((1, seq_len), decoder_input_ids.0.clone())
            .map_err(|e| ReaderError::Internal(format!("Shape error: {}", e)))?;
        let mask = ndarray::Array2::from_shape_vec((1, enc_seq_len), attention_mask.0.clone())
            .map_err(|e| ReaderError::Internal(format!("Shape error: {}", e)))?;

        let inputs = ort::inputs![
            "input_ids" => Value::from_array(input_ids).map_err(|e| ReaderError::Onnx(e.to_string()))?,
            "encoder_hidden_states" => encoder_hidden,
            "encoder_attention_mask" => Value::from_array(mask).map_err(|e| ReaderError::Onnx(e.to_string()))?,
        ];

        let mut outputs = self.decoder.run(inputs)
            .map_err(|e| ReaderError::Onnx(format!("Decoder step one failed: {}", e)))?;

        let logits_value = outputs.remove("logits")
            .ok_or_else(|| ReaderError::Onnx("Missing logits".into()))?;
        let logits_array = logits_value.try_extract_tensor::<f32>()
            .map_err(|e| ReaderError::Onnx(format!("Logits extract failed: {}", e)))?;
        
        // Shape is [1, seq_len, vocab_size]. We only want the last token's logits.
        let vocab_size = logits_array.shape()[2];
        let start_idx = (seq_len - 1) * vocab_size;
        let logits_vec: Vec<f32> = logits_array.iter().skip(start_idx).take(vocab_size).copied().collect();

        let cache = PopulatedCache::update_from_outputs(outputs)?;

        Ok((logits_vec, cache))
    }

    pub fn run_decoder_step_with_past(
        &self,
        decoder_input_ids: &TokenIds,
        encoder_hidden: &Value,
        attention_mask: &AttentionMask,
        cache: PopulatedCache, // Ownership moved here
    ) -> Result<(Vec<f32>, PopulatedCache), ReaderError> {
        let seq_len = decoder_input_ids.0.len();
        let enc_seq_len = attention_mask.0.len();

        let input_ids = ndarray::Array2::from_shape_vec((1, seq_len), decoder_input_ids.0.clone())
            .map_err(|e| ReaderError::Internal(format!("Shape error: {}", e)))?;
        let mask = ndarray::Array2::from_shape_vec((1, enc_seq_len), attention_mask.0.clone())
            .map_err(|e| ReaderError::Internal(format!("Shape error: {}", e)))?;

        // Build base inputs
        let input_ids_val = Value::from_array(input_ids).map_err(|e| ReaderError::Onnx(e.to_string()))?;
        let mask_val = Value::from_array(mask).map_err(|e| ReaderError::Onnx(e.to_string()))?;

        let mut inputs = ort::inputs![
            "input_ids" => input_ids_val,
            "encoder_hidden_states" => encoder_hidden,
            "encoder_attention_mask" => mask_val,
        ];

        // Append the 24 cache tensors
        for (name, tensor) in cache.build_inputs() {
            inputs.push((name, tensor));
        }

        let mut outputs = self.decoder_with_past.run(inputs)
            .map_err(|e| ReaderError::Onnx(format!("Decoder with past failed: {}", e)))?;

        let logits_value = outputs.remove("logits")
            .ok_or_else(|| ReaderError::Onnx("Missing logits".into()))?;
        let logits_array = logits_value.try_extract_tensor::<f32>()
            .map_err(|e| ReaderError::Onnx(format!("Logits extract failed: {}", e)))?;
            
        let vocab_size = logits_array.shape()[2];
        let start_idx = (seq_len - 1) * vocab_size;
        let logits_vec: Vec<f32> = logits_array.iter().skip(start_idx).take(vocab_size).copied().collect();

        let new_cache = PopulatedCache::update_from_outputs(outputs)?;

        Ok((logits_vec, new_cache))
    }
}
