//! End-to-end integration test for the full ML pipeline.
//!
//! This test exercises the entire flow: tokenizer, ONNX encoder, ONNX decoder,
//! KV cache management, Trie masking, and regex fallback.
//!
//! Because it requires the real 500MB ONNX models, it checks for their
//! existence and skips gracefully if they are not present (e.g., in CI).

use std::path::Path;
use hfm_text_filter::filter::{BartDetoxFilter, TextFilter};
use hfm_text_filter::types::RawChunk;

#[test]
fn test_full_pipeline() {
    let model_dir = Path::new("models/bart-base-detox/onnx");
    
    // 1. Check if the model exists
    if !model_dir.join("encoder_model.onnx").exists() {
        println!("Skipping test: ONNX models not found at {:?}.", model_dir);
        return;
    }

    // 2. Setup the filter with a small banned word list
    let banned_words = vec![
        "shit".to_string(),
        "fuck".to_string(),
    ];
    
    // max_new_tokens = 50, eos_token_id = 2, decoder_start_token_id = 2
    let mut filter = BartDetoxFilter::new(model_dir, banned_words, 50, 2, 2)
        .expect("Failed to initialize BartDetoxFilter");

    // 3. Test with a toxic sentence
    // The model should either rewrite this naturally or the fallback should catch it.
    let input = "This is absolute shit and I hate it.";
    let raw_chunk = RawChunk::new(input);
    
    println!("Running inference on: {}", input);
    println!("This may take a few seconds to load the model...");

    let result = filter.process(&raw_chunk).expect("Inference failed");
    let output = result.data;
    
    println!("Output: {}", output);

    // 4. Assert the output is clean
    // The Trie mask should force the model to pick different tokens, or the fallback should redact.
    assert!(!output.to_lowercase().contains("shit"), "Output still contains banned word: {}", output);
    assert!(!output.to_lowercase().contains("fuck"), "Output still contains banned word: {}", output);

    // 5. Assert the output is not empty (the model actually generated something)
    assert!(!output.trim().is_empty(), "Output is empty");
}
