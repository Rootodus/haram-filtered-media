use anyhow::{Result, anyhow};
use std::sync::OnceLock;
use tokenizers::Tokenizer;

pub static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();

pub fn init_tokenizer(path: &str) -> Result<()> {
    let tokenizer = Tokenizer::from_file(path).map_err(|e| anyhow!(e))?;
    TOKENIZER
        .set(tokenizer)
        .map_err(|_| anyhow!("Tokenizer already initialized"))?;
    Ok(())
}

pub fn tokenize(text: &str, max_len: usize) -> (Vec<i64>, Vec<i64>) {
    let tokenizer = TOKENIZER.get().expect("Tokenizer not initialized");
    let encoding = tokenizer.encode(text, true).expect("Tokenization failed");
    let mut input_ids = vec![0i64; max_len];
    let mut attention_mask = vec![0i64; max_len];
    let tokens = encoding.get_ids();
    let len = tokens.len().min(max_len);
    for i in 0..len {
        input_ids[i] = tokens[i] as i64;
        attention_mask[i] = 1;
    }
    (input_ids, attention_mask)
}
