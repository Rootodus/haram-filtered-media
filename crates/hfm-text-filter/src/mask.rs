//! The Trie logic for preemptive logit masking.

use crate::tokenizer::BartTokenizer;
use std::collections::HashMap;

struct TrieNode {
    children: HashMap<u32, TrieNode>,
    is_banned_end: bool,
}

impl TrieNode {
    fn new() -> Self {
        Self { children: HashMap::new(), is_banned_end: false }
    }
}

pub struct TokenMask {
    root: TrieNode,
}

impl TokenMask {
    /// Build the Trie from the static banned words list.
    pub fn new(banned_words: &[String], tokenizer: &BartTokenizer) -> Self {
        todo!("Build the Trie using tokenizer.encode()")
    }

    /// Modify the raw logits slice in-place, setting forbidden tokens to -inf.
    pub fn apply_mask(&self, logits: &mut [f32], recent_tokens: &[i64]) {
        todo!("Check Trie against recent_tokens and mask logits")
    }

    /// Find the token ID with the highest probability (argmax).
    pub fn select_next_token(logits: &[f32]) -> i64 {
        todo!("Loop through logits and return the index of the highest value")
    }
}