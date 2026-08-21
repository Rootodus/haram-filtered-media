//! ONNX session builder with configurable execution providers.
//!
//! This module replaces the previous hardcoded provider selection with a
//! generic builder that accepts a `SessionConfig`. Both video and audio
//! models use the same logic.

use crate::ml::{ExecutionProvider, SessionConfig};
use anyhow::{Result, anyhow};
use ort::session::Session;

/// Build an ONNX session with the given configuration.
///
/// This is the primary function for creating sessions. It handles all
/// execution provider registration and fallback logic.
pub fn build_session(path: &str, config: SessionConfig) -> Result<Session> {
    let mut builder =
        Session::builder().map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
    builder = builder
        .with_optimization_level(config.optimization_level)
        .map_err(|e| anyhow!("Failed to set optimization level: {:?}", e))?;
    builder = builder
        .with_intra_threads(config.intra_threads)
        .map_err(|e| anyhow!("Failed to set intra threads: {}", e))?;
    builder = builder
        .with_inter_threads(config.inter_threads)
        .map_err(|e| anyhow!("Failed to set inter threads: {}", e))?;

    if config.disable_cpu_fallback {
        builder = builder
            .with_disable_cpu_fallback()
            .map_err(|e| anyhow!("Failed to disable CPU fallback: {}", e))?;
    }

    // Register the requested provider.
    match config.provider {
        ExecutionProvider::Cpu => {
            builder = builder
                .with_execution_providers([ort::ep::CPU::default().build()])
                .map_err(|e| anyhow!("Failed to set CPU provider: {}", e))?;
        }
        ExecutionProvider::DirectML => {
            #[cfg(target_os = "windows")]
            {
                use ort::ep::DirectML;
                builder = builder
                    .with_execution_providers([DirectML::default().build()])
                    .map_err(|e| anyhow!("Failed to set DirectML provider: {}", e))?;
            }
            #[cfg(not(target_os = "windows"))]
            {
                return Err(anyhow!("DirectML is only supported on Windows"));
            }
        }
        ExecutionProvider::OpenVINO { device } => {
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            {
                use ort::ep::OpenVINO;
                builder = builder
                    .with_execution_providers([OpenVINO::default()
                        .with_device_type(&device)
                        .build()])
                    .map_err(|e| anyhow!("Failed to set OpenVINO provider: {}", e))?;
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            {
                return Err(anyhow!("OpenVINO is only supported on Linux and Windows"));
            }
        }
        ExecutionProvider::CoreML => {
            #[cfg(target_vendor = "apple")]
            {
                use ort::ep::CoreML;
                builder = builder
                    .with_execution_providers([CoreML::default().build()])
                    .map_err(|e| anyhow!("Failed to set CoreML provider: {}", e))?;
            }
            #[cfg(not(target_vendor = "apple"))]
            {
                return Err(anyhow!("CoreML is only supported on Apple platforms"));
            }
        }
    }

    builder
        .commit_from_file(path)
        .map_err(|e| anyhow!("Failed to commit session: {}", e))
}

/// Convenience wrapper for video models using the default video config.
pub fn init_session(path: &str) -> Result<Session> {
    let config = SessionConfig::video_default();
    build_session(path, config)
}
