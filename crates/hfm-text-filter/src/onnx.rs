//! Raw ONNX session execution, isolated from tokenization and masking.

use crate::cache::PopulatedCache;
use crate::types::tensor::{AttentionMask, TokenIds};
use crate::types::ReaderError;
use ort::session::{Session, SessionInputValue};
use ort::value::Value;
use std::borrow::Cow;
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

    pub fn run_encoder(&mut self, ids: &TokenIds, mask: &AttentionMask) -> Result<Value, ReaderError> {
        let seq_len = ids.0.len();
        
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
        &mut self,
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
        
        let (shape, data) = logits_value.try_extract_tensor::<f32>()
            .map_err(|e| ReaderError::Onnx(format!("Logits extract failed: {}", e)))?;
        
        // Shape is [1, seq_len, vocab_size]. We only want the last token's logits.
        let vocab_size = shape[2] as usize;
        let start_idx = (seq_len - 1) * vocab_size;
        let logits_vec: Vec<f32> = data.iter().skip(start_idx).take(vocab_size).copied().collect();

        let cache = PopulatedCache::update_from_outputs(outputs)?;

        Ok((logits_vec, cache))
    }

    pub fn run_decoder_step_with_past(
        &mut self,
        decoder_input_ids: &TokenIds,
        attention_mask: &AttentionMask,
        cache: PopulatedCache,
    ) -> Result<(Vec<f32>, PopulatedCache), ReaderError> {
        let seq_len = decoder_input_ids.0.len();
        let enc_seq_len = attention_mask.0.len();

        let input_ids = ndarray::Array2::from_shape_vec((1, seq_len), decoder_input_ids.0.clone())
            .map_err(|e| ReaderError::Internal(format!("Shape error: {}", e)))?;
        let mask = ndarray::Array2::from_shape_vec((1, enc_seq_len), attention_mask.0.clone())
            .map_err(|e| ReaderError::Internal(format!("Shape error: {}", e)))?;

        // Build base inputs. Note: `encoder_hidden_states` is NOT an input for
        // `decoder_with_past_model.onnx` — the encoder's K/V are already in the
        // past cache, so it would be redundant.
        let input_ids_val = Value::from_array(input_ids).map_err(|e| ReaderError::Onnx(e.to_string()))?;
        let mask_val = Value::from_array(mask).map_err(|e| ReaderError::Onnx(e.to_string()))?;

        // Build inputs manually so all entries (owned base + borrowed cache)
        // share a single lifetime, letting `cache` be consumed after `run()`.
        let mut inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> = Vec::with_capacity(26);
        inputs.push((Cow::Borrowed("input_ids"), SessionInputValue::Owned(input_ids_val)));
        inputs.push((Cow::Borrowed("encoder_attention_mask"), SessionInputValue::Owned(mask_val)));
        inputs.extend(cache.build_inputs());

        let mut outputs = self.decoder_with_past.run(inputs)
            .map_err(|e| ReaderError::Onnx(format!("Decoder with past failed: {}", e)))?;

        let logits_value = outputs.remove("logits")
            .ok_or_else(|| ReaderError::Onnx("Missing logits".into()))?;
        
        let (shape, data) = logits_value.try_extract_tensor::<f32>()
            .map_err(|e| ReaderError::Onnx(format!("Logits extract failed: {}", e)))?;
            
        let vocab_size = shape[2] as usize;
        let start_idx = (seq_len - 1) * vocab_size;
        let logits_vec: Vec<f32> = data.iter().skip(start_idx).take(vocab_size).copied().collect();

        // decoder_with_past only outputs the 12 decoder tensors.
        // The 12 encoder tensors are preserved from the previous cache.
        let new_cache = cache.update_from_past_outputs(outputs)?;

        Ok((logits_vec, new_cache))
    }
}
