pub mod inference;
pub mod network;
pub mod protocol;
pub mod render;
pub mod state;

// Include generated FlatBuffer bindings
include!(concat!(env!("OUT_DIR"), "/flatbuffers/mod.rs"));
