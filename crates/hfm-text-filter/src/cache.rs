//! Decoder KV cache management using the Typestate pattern.

use ort::value::Value;

/// Passed to the first decoder step. Guarantees no cache exists yet.
pub struct EmptyCache;

impl EmptyCache {
    pub fn new() -> Self {
        Self
    }
}

/// Passed to subsequent decoder steps. Guarantees 24 tensors exist.
pub struct PopulatedCache {
    /// 6 layers * 4 tensors (decoder.key, decoder.value, encoder.key, encoder.value)
    pub tensors: [Value; 24],
}

impl PopulatedCache {
    /// Extract the 24 present tensors from the ONNX output.
    pub fn update_from_outputs(outputs: &ort::session::SessionOutputs) -> Self {
        todo!("Extract present.* tensors into an array of 24")
    }

    /// Format the cached tensors into a named list for the ONNX inputs! macro.
    pub fn build_inputs(&self) -> Vec<(&'static str, Value)> {
        todo!("Map the 24 tensors to their expected ONNX input names like 'past_key_values.0.decoder.key'")
    }
}