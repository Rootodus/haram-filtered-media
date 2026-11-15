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
            if let Ok(token_ids) = tokenizer.encode(word) {
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
        // We check every possible suffix of the recent tokens.
        // For example, if history is [10, 20, 30], we check:
        // [10, 20, 30], [20, 30], [30]
        // If any suffix matches a prefix in the Trie, we mask the completing token.
        for start_idx in 0..recent_tokens.len() {
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
    pub fn select_next_token(logits: &[f32]) -> crate::types::ReaderResult<i64> {
        let mut max_idx = 0;
        let mut max_val = f32::NEG_INFINITY;

        for (i, &val) in logits.iter().enumerate() {
            if val > max_val {
                max_val = val;
                max_idx = i;
            }
        }

        if max_val == f32::NEG_INFINITY {
            return Err(crate::types::ReaderError::Mask("Dead end: all tokens masked".into()));
        }

        Ok(max_idx as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::tensor::TokenIds;
    use crate::types::{ReaderError, ReaderResult};
    use std::path::Path;

    // A fake tokenizer for testing without loading the real 500MB model.
    struct MockTokenizer;
    
    impl MockTokenizer {
        fn new() -> Self { Self }
        
        // Simulates encoding: "bad" -> [10, 20], "word" -> [30]
        fn encode_mock(&self, text: &str) -> TokenIds {
            match text {
                "bad" => TokenIds(vec![10, 20]),
                "word" => TokenIds(vec![30]),
                _ => TokenIds(vec![99]),
            }
        }
    }

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
        
        // History is [10] (prefix of "bad").
        // The next token should be masked if it's 20.
        let history = vec![10];
        mask.apply_mask(&mut logits, &history);
        
        assert_eq!(logits[20], f32::NEG_INFINITY);
        assert_eq!(logits[30], 0.0); // 30 is untouched
    }

    #[test]
    fn test_apply_mask_exact_match() {
        let mask = build_test_mask();
        let mut logits = vec![0.0; 100];
        
        // History ends with [30], which is "word".
        // The next token shouldn't be masked unless it forms a longer ban.
        // Since [30] is a leaf node, there are no children to mask.
        let history = vec![10, 30];
        mask.apply_mask(&mut logits, &history);
        
        // Nothing should be -inf.
        for val in logits {
            assert_ne!(val, f32::NEG_INFINITY);
        }
    }

    #[test]
    fn test_apply_mask_no_match() {
        let mask = build_test_mask();
        let mut logits = vec![0.0; 100];
        
        let history = vec![99, 99];
        mask.apply_mask(&mut logits, &history);
        
        // Nothing should be masked.
        for val in logits {
            assert_ne!(val, f32::NEG_INFINITY);
        }
    }
}