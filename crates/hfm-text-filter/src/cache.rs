//! Decoder KV cache management using the Typestate pattern.

use crate::types::ReaderError;
use ort::session::SessionOutputs;
use ort::value::Value;

/// Passed to the first decoder step. Guarantees no cache exists yet.
pub struct EmptyCache;

impl EmptyCache {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EmptyCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Passed to subsequent decoder steps. Guarantees 24 tensors exist.
pub struct PopulatedCache {
    pub tensors: [Value; 24],
}

// The exact ONNX output names for the 6 layers (BART-base has 6 decoder layers).
const OUTPUT_NAMES: [&str; 24] = [
    "present.0.decoder.key",   "present.0.decoder.value",   "present.0.encoder.key",   "present.0.encoder.value",
    "present.1.decoder.key",   "present.1.decoder.value",   "present.1.encoder.key",   "present.1.encoder.value",
    "present.2.decoder.key",   "present.2.decoder.value",   "present.2.encoder.key",   "present.2.encoder.value",
    "present.3.decoder.key",   "present.3.decoder.value",   "present.3.encoder.key",   "present.3.encoder.value",
    "present.4.decoder.key",   "present.4.decoder.value",   "present.4.encoder.key",   "present.4.encoder.value",
    "present.5.decoder.key",   "present.5.decoder.value",   "present.5.encoder.key",   "present.5.encoder.value",
];

// The exact ONNX input names for the next generation step.
const INPUT_NAMES: [&str; 24] = [
    "past_key_values.0.decoder.key",   "past_key_values.0.decoder.value",   "past_key_values.0.encoder.key",   "past_key_values.0.encoder.value",
    "past_key_values.1.decoder.key",   "past_key_values.1.decoder.value",   "past_key_values.1.encoder.key",   "past_key_values.1.encoder.value",
    "past_key_values.2.decoder.key",   "past_key_values.2.decoder.value",   "past_key_values.2.encoder.key",   "past_key_values.2.encoder.value",
    "past_key_values.3.decoder.key",   "past_key_values.3.decoder.value",   "past_key_values.3.encoder.key",   "past_key_values.3.encoder.value",
    "past_key_values.4.decoder.key",   "past_key_values.4.decoder.value",   "past_key_values.4.encoder.key",   "past_key_values.4.encoder.value",
    "past_key_values.5.decoder.key",   "past_key_values.5.decoder.value",   "past_key_values.5.encoder.key",   "past_key_values.5.encoder.value",
];

impl PopulatedCache {
    /// Extract the 24 present tensors from the ONNX output.
    /// Consumes the SessionOutputs because `Value` is not cloneable.
    pub fn update_from_outputs(mut outputs: SessionOutputs) -> Result<Self, ReaderError> {
        let mut tensor_vec = Vec::with_capacity(24);

        for &name in &OUTPUT_NAMES {
            let tensor = outputs
                .remove(name)
                .ok_or_else(|| ReaderError::Onnx(format!("Missing cache output: {}", name)))?;

            tensor_vec.push(tensor);
        }

        let tensors: [Value; 24] = tensor_vec.try_into().map_err(|_| {
            ReaderError::Internal("Cache array size mismatch".to_string())
        })?;

        Ok(Self { tensors })
    }

    /// Update the cache after a `decoder_with_past_model.onnx` step.
    ///
    /// `decoder_with_past_model.onnx` outputs only 12 tensors (6 layers × 2 =
    /// decoder key/value). The 12 encoder key/value tensors were computed once in
    /// step 1 and do not change; they are carried over from `self`.
    ///
    /// Consumes `self` because `Value` is not cloneable.
    pub fn update_from_past_outputs(
        self,
        mut outputs: SessionOutputs,
    ) -> Result<Self, ReaderError> {
        // Consume the old array into an iterator so we can move the encoder
        // tensors out of it one at a time.
        let mut old_iter = self.tensors.into_iter();
        let mut new_tensors: Vec<Value> = Vec::with_capacity(24);

        for layer in 0..6u32 {
            // Discard the old decoder K/V (they are being replaced).
            let _old_dkey = old_iter.next().expect("Cache length mismatch");
            let _old_dval = old_iter.next().expect("Cache length mismatch");
            // Preserve the old encoder K/V (they do not change).
            let old_ekey = old_iter.next().expect("Cache length mismatch");
            let old_eval = old_iter.next().expect("Cache length mismatch");

            // Pull new decoder K/V from the current step's outputs.
            let dkey_name = format!("present.{}.decoder.key", layer);
            let dkey = outputs
                .remove(&dkey_name)
                .ok_or_else(|| ReaderError::Onnx(format!("Missing {}", dkey_name)))?;

            let dval_name = format!("present.{}.decoder.value", layer);
            let dval = outputs
                .remove(&dval_name)
                .ok_or_else(|| ReaderError::Onnx(format!("Missing {}", dval_name)))?;

            // Reassemble in the array layout expected by INPUT_NAMES:
            // [dkey, dval, ekey, eval] per layer.
            new_tensors.push(dkey);
            new_tensors.push(dval);
            new_tensors.push(old_ekey);
            new_tensors.push(old_eval);
        }

        let tensors: [Value; 24] = new_tensors
            .try_into()
            .map_err(|_| ReaderError::Internal("Cache array size mismatch".to_string()))?;

        Ok(Self { tensors })
    }

    /// Format the cached tensors into a named list for the ONNX inputs.
    /// Borrows `self` so the encoder tensors remain accessible after `run()`
    /// returns (they are needed again for `update_from_past_outputs`).
    pub fn build_inputs(&self) -> Vec<(std::borrow::Cow<'static, str>, ort::session::SessionInputValue<'_>)> {
        INPUT_NAMES
            .iter()
            .copied()
            .zip(self.tensors.iter())
            .map(|(name, tensor)| {
                (
                    std::borrow::Cow::Borrowed(name),
                    ort::session::SessionInputValue::View(tensor.view()),
                )
            })
            .collect()
    }
}
