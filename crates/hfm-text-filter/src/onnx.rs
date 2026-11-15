//! Raw ONNX session execution, isolated from tokenization and masking.

use crate::cache::{EmptyCache, PopulatedCache};
use crate::types::ReaderError;
use crate::types::tensor::{AttentionMask, TokenIds};
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
        todo!("Load encoder_model.onnx, decoder_model.onnx, decoder_with_past_model.onnx")
    }

    pub fn run_encoder(&self, ids: &TokenIds, mask: &AttentionMask) -> Result<Value, ReaderError> {
        todo!("Run encoder session, return last_hidden_state")
    }

    pub fn run_decoder_step_one(
        &self,
        decoder_input_ids: &TokenIds,
        encoder_hidden: &Value,
        attention_mask: &AttentionMask,
    ) -> Result<(Vec<f32>, PopulatedCache), ReaderError> {
        todo!("Run decoder session, return raw logits and new cache")
    }

    pub fn run_decoder_step_with_past(
        &self,
        decoder_input_ids: &TokenIds,
        encoder_hidden: &Value,
        attention_mask: &AttentionMask,
        cache: &PopulatedCache,
    ) -> Result<(Vec<f32>, PopulatedCache), ReaderError> {
        todo!("Run decoder_with_past session using cache inputs, return raw logits and new cache")
    }
}