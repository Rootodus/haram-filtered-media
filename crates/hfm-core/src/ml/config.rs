//! ONNX session configuration.
//!
//! This module defines the configuration struct and default settings for
//! video and audio models. The same builder logic is used for both.

use ort::session::builder::GraphOptimizationLevel;

/// Execution provider to use for inference.
#[derive(Debug, Clone)]
pub enum ExecutionProvider {
    /// CPU execution provider.
    Cpu,
    /// DirectML (Windows only).
    DirectML,
    /// OpenVINO with a specific device (e.g., "GPU", "CPU").
    OpenVINO { device: String },
    /// CoreML (Apple only).
    CoreML,
}

/// Configuration for an ONNX session.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub provider: ExecutionProvider,
    pub intra_threads: usize,
    pub inter_threads: usize,
    pub optimization_level: GraphOptimizationLevel,
    pub disable_cpu_fallback: bool,
}

impl SessionConfig {
    /// Default configuration for the video model (PPHumanSeg).
    /// Uses the best available hardware backend for the platform.
    pub fn video_default() -> Self {
        #[cfg(target_os = "windows")]
        let provider = ExecutionProvider::DirectML;
        #[cfg(target_os = "linux")]
        let provider = ExecutionProvider::OpenVINO {
            device: "GPU".to_string(),
        };
        #[cfg(target_vendor = "apple")]
        let provider = ExecutionProvider::CoreML;
        // Fallback for other platforms (e.g., wasm) – use CPU.
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_vendor = "apple")))]
        let provider = ExecutionProvider::Cpu;

        Self {
            provider,
            intra_threads: 1,
            inter_threads: 1,
            optimization_level: GraphOptimizationLevel::Level1,
            disable_cpu_fallback: true,
        }
    }

    /// Default configuration for the audio model (HT-Demucs).
    /// Uses CPU with half of the available logical cores, capped at 4.
    pub fn audio_default() -> Self {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        // Use half the cores, at least 1, at most 4.
        let intra = (cores / 2).max(1).min(4);
        Self {
            provider: ExecutionProvider::Cpu,
            intra_threads: intra,
            inter_threads: 1,
            optimization_level: GraphOptimizationLevel::Level1,
            disable_cpu_fallback: false,
        }
    }
}
