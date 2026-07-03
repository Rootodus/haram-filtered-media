pub mod inference;
pub mod network;
pub mod parser;
pub mod protocol;
pub mod render;
pub mod state;
pub mod tokenizer;

// Include generated FlatBuffer bindings
include!(concat!(env!("OUT_DIR"), "/flatbuffers/mod.rs"));
