//! Newtypes to prevent parameter-swapping bugs when calling ONNX.

#[derive(Debug, Clone)]
pub struct TokenIds(pub Vec<i64>);

#[derive(Debug, Clone)]
pub struct AttentionMask(pub Vec<i64>);