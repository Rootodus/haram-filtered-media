pub mod protocol;

// Include generated FlatBuffer bindings
include!(concat!(env!("OUT_DIR"), "/flatbuffers/mod.rs"));

// Re-export from the generated structure into the library root
// Path: <namespace_mod> :: <Type>
pub use schema::{DomNode, Metadata, Rect};
