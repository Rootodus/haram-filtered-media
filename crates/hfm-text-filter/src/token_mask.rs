//! The Trie logic for preemptive logit masking.

use crate::tokenizer::BartTokenizer;
use std::collections::HashMap;

/// A node in the Trie.
struct TrieNode {
    children: HashMap<u32, TrieNode>,
    is_banned_end: bool,
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            is_banned_end: false,
        }
    }
}

pub struct TokenMask {
    root: TrieNode,
}

impl TokenMask {
    /// Build the Trie from the static banned words list.
    pub fn new(banned_words: &[String], tokenizer: &BartTokenizer) -> Self {
        let mut root = TrieNode::new();
        
        for word in banned_words {
            if let Ok(token_ids) = tokenizer.encode_no_special(word) {
                if token_ids.0.is_empty() {
                    continue;
                }
                let mut current = &mut root;
                for &id in &token_ids.0 {
                    current = current.children.entry(id as u32).or_insert_with(TrieNode::new);
                }
                current.is_banned_end = true;
            }
        }
        
        Self { root }
    }

    /// Helper: Walk the Trie from the root with the given sequence.
    /// Returns the final node if the exact path exists.
    fn walk(&self, tokens: &[i64]) -> Option<&TrieNode> {
        let mut current = &self.root;
        for &token in tokens {
            current = current.children.get(&(token as u32))?;
        }
        Some(current)
    }

    /// Modify the raw logits slice in-place, setting forbidden tokens to -inf.
    pub fn apply_mask(&self, logits: &mut [f32], recent_tokens: &[i64]) {
        // We check every possible suffix of the recent tokens, INCLUDING the empty suffix.
        // The empty suffix (walking to the root) is what masks single-token banned words.
        // This matches the behavior of HuggingFace's NoBadWordsLogitsProcessor.
        for start_idx in 0..=recent_tokens.len() {
            let suffix = &recent_tokens[start_idx..];
            if let Some(node) = self.walk(suffix) {
                for (token_id, child) in &node.children {
                    if child.is_banned_end {
                        let idx = *token_id as usize;
                        if idx < logits.len() {
                            logits[idx] = f32::NEG_INFINITY;
                        }
                    }
                }
            }
        }
    }

    /// Find the token ID with the highest probability (argmax).
    /// Returns an error if the model is in a dead end (all logits are -inf).
    pub fn select_next_token(logits: &[f32]) -> crate::types::FilterResult<i64> {
        let mut max_idx = 0;
        let mut max_val = f32::NEG_INFINITY;

        for (i, &val) in logits.iter().enumerate() {
            if val > max_val {
                max_val = val;
                max_idx = i;
            }
        }

        if max_val == f32::NEG_INFINITY {
            return Err(crate::types::FilterError::Mask("Dead end: all tokens masked".into()));
        }

        Ok(max_idx as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to build a mask using our mock data
    fn build_test_mask() -> TokenMask {
        let mut root = TrieNode::new();
        
        // Manually build the Trie: [10, 20] and [30]
        let mut current = &mut root;
        current = current.children.entry(10).or_insert_with(TrieNode::new);
        current = current.children.entry(20).or_insert_with(TrieNode::new);
        current.is_banned_end = true;

        let mut current = &mut root;
        current = current.children.entry(30).or_insert_with(TrieNode::new);
        current.is_banned_end = true;

        TokenMask { root }
    }

    #[test]
    fn test_select_next_token() {
        let logits = vec![0.1, 0.5, 0.2, 0.8, 0.0];
        // Highest is at index 3 (value 0.8)
        assert_eq!(TokenMask::select_next_token(&logits).unwrap(), 3);
    }

    #[test]
    fn test_select_next_token_dead_end() {
        let logits = vec![f32::NEG_INFINITY; 10];
        let result = TokenMask::select_next_token(&logits);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_mask_completes_sequence() {
        let mask = build_test_mask();
        let mut logits = vec![0.0; 100];

        // History is [10] (prefix of banned word [10, 20]).
        // Token 30 is a single-token banned word.
        let history = vec![10];
        mask.apply_mask(&mut logits, &history);

        // Token 20 completes the sequence [10, 20], so it's masked.
        assert_eq!(logits[20], f32::NEG_INFINITY);
        // Token 30 is a single-token banned word, so it's masked too.
        assert_eq!(logits[30], f32::NEG_INFINITY);
        // Token 10 is a prefix of the banned word [10, 20], not a banned word itself.
        assert_eq!(logits[10], 0.0);
    }

    #[test]
    fn test_apply_mask_exact_match() {
        let mask = build_test_mask();
        let mut logits = vec![0.0; 100];

        // History is [10, 30]. Token 30 is a single-token banned word.
        let history = vec![10, 30];
        mask.apply_mask(&mut logits, &history);

        // Token 30 is a single-token banned word and is masked at every step
        // (matching HF's NoBadWordsLogitsProcessor behavior).
        assert_eq!(logits[30], f32::NEG_INFINITY);
        // Token 10 is not a banned word.
        assert_ne!(logits[10], f32::NEG_INFINITY);
        // Token 20 is not masked because history doesn't end in 10.
        assert_ne!(logits[20], f32::NEG_INFINITY);
    }

    #[test]
    fn test_apply_mask_no_match() {
        let mask = build_test_mask();
        let mut logits = vec![0.0; 100];

        // History unrelated to any banned sequence.
        let history = vec![99, 99];
        mask.apply_mask(&mut logits, &history);

        // Token 10 is the prefix of banned word [10, 20], not a banned word itself.
        assert_ne!(logits[10], f32::NEG_INFINITY);
        // Token 20 is the second half of [10, 20], but history doesn't end in 10.
        assert_ne!(logits[20], f32::NEG_INFINITY);
        // Token 30 is a single-token banned word, always masked regardless of history.
        assert_eq!(logits[30], f32::NEG_INFINITY);
    }

    #[test]
    fn test_token_mask_new_with_real_tokenizer() {
        let model_dir = std::path::Path::new("models/bart-base-detox/onnx");
        if !model_dir.join("tokenizer.json").exists() {
            println!("Skipping test: tokenizer.json not found. Run from crate root with models downloaded.");
            return;
        }

        let tokenizer = crate::tokenizer::BartTokenizer::new(model_dir).unwrap();
        let banned = vec!["shit".to_string(), "fuck".to_string()];
        let mask = TokenMask::new(&banned, &tokenizer);

        // We need a large logits array. Get vocab size from tokenizer.
        let mut logits = vec![0.0; tokenizer.vocab_size()];

        // Encode "shit" without special tokens to get the exact token IDs
        let shit_tokens = tokenizer.encode_no_special("shit").unwrap().0;
        assert!(!shit_tokens.is_empty(), "Tokenization of 'shit' failed");

        // Apply mask using all but the last token as history
        let history = &shit_tokens[..shit_tokens.len() - 1];
        mask.apply_mask(&mut logits, history);

        // The last token of "shit" should now be masked (-inf)
        let last_token = shit_tokens[shit_tokens.len() - 1] as usize;
        assert_eq!(
            logits[last_token], 
            f32::NEG_INFINITY, 
            "Expected last token of 'shit' to be masked"
        );

        // An unrelated token (e.g., the first token of "hello") should not be masked
        let hello_tokens = tokenizer.encode_no_special("hello").unwrap().0;
        let hello_first = hello_tokens[0] as usize;
        assert_ne!(
            logits[hello_first], 
            f32::NEG_INFINITY, 
            "Unrelated token should not be masked"
        );
    }
}
