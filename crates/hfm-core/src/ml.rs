//! Machine learning module: ONNX session management and model implementations.

pub mod config;
pub mod demucs;
pub mod engine;
pub mod peopleseg;

pub use config::{ExecutionProvider, SessionConfig};
pub use demucs::{DemucsConfig, spawn_demucs_worker};
pub use engine::{build_session, init_session};
pub use peopleseg::PeopleSegFilter;
