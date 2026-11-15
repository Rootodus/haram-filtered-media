//! Raw ONNX session execution, isolated from tokenization and masking.

use crate::cache::{EmptyCache, PopulatedCache};
use crate::types::tensor::{AttentionMask, TokenIds};
use crate::types::{ReaderError, ReaderResult};
use ort::session::{Session, SessionOutputs};
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

    /// Run the encoder to get hidden states.
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

        let outputs = self.encoder.run(inputs)
            .map_err(|e| ReaderError::Onnx(format!("Encoder run failed: {}", e)))?;

        let hidden = outputs.remove("last_hidden_state")
            .ok_or_else(|| ReaderError::Onnx("Missing last_hidden_state".into()))?;

        Ok(hidden)
    }

    /// Run the decoder for the very first token (no past cache).
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

        // Extract logits: shape is [1, 1, vocab_size]
        let logits_value = outputs.remove("logits")
            .ok_or_else(|| ReaderError::Onnx("Missing logits".into()))?;
        let logits_array = logits_value.try_extract_tensor::<f32>()
            .map_err(|e| ReaderError::Onnx(format!("Logits extract failed: {}", e)))?;
        
        // Flatten the [1, 1, vocab_size] array into a Vec<f32>
        let logits_vec = logits_array.view().iter().copied().collect();

        // Extract the 24 KV cache tensors
        let cache = PopulatedCache::update_from_outputs(outputs)?;

        Ok((logits_vec, cache))
    }

    /// Run the decoder for subsequent tokens (with past cache).
    pub fn run_decoder_step_with_past(
        &self,
        decoder_input_ids: &TokenIds,
        encoder_hidden: &Value,
        attention_mask: &AttentionMask,
        cache: &PopulatedCache,
    ) -> Result<(Vec<f32>, PopulatedCache), ReaderError> {
        let seq_len = decoder_input_ids.0.len();
        let enc_seq_len = attention_mask.0.len();

        let input_ids = ndarray::Array2::from_shape_vec((1, seq_len), decoder_input_ids.0.clone())
            .map_err(|e| ReaderError::Internal(format!("Shape error: {}", e)))?;
        let mask = ndarray::Array2::from_shape_vec((1, enc_seq_len), attention_mask.0.clone())
            .map_err(|e| ReaderError::Internal(format!("Shape error: {}", e)))?;

        // Start with the base inputs
        let mut inputs = ort::inputs![
            "input_ids" => Value::from_array(input_ids).map_err(|e| ReaderError::Onnx(e.to_string()))?,
            "encoder_hidden_states" => encoder_hidden,
            "encoder_attention_mask" => Value::from_array(mask).map_err(|e| ReaderError::Onnx(e.to_string()))?,
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
        let logits_vec = logits_array.view().iter().copied().collect();

        let new_cache = PopulatedCache::update_from_outputs(outputs)?;

        Ok((logits_vec, new_cache))
    }
}