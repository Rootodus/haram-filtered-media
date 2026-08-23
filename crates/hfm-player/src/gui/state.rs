//! Shared state between UI and pipeline.

use std::path::PathBuf;

/// Available execution backends for ONNX models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    #[default]
    Cpu,
    DirectML,
    OpenVINO,
    CoreML,
}

/// Playback state (playing or paused).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self::Paused
    }
}

/// Volume value clamped to 0..100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume(u8);

impl Volume {
    pub const MIN: u8 = 0;
    pub const MAX: u8 = 100;

    /// Create a new volume, clamping to the valid range.
    pub fn new(value: u8) -> Self {
        Self(value.clamp(Self::MIN, Self::MAX))
    }

    /// Get the raw value.
    pub fn get(&self) -> u8 {
        self.0
    }

    /// Increase volume by a step, clamping at MAX.
    pub fn step_up(&mut self, step: u8) {
        self.0 = (self.0 + step).min(Self::MAX);
    }

    /// Decrease volume by a step, clamping at MIN.
    pub fn step_down(&mut self, step: u8) {
        self.0 = (self.0 - step).max(Self::MIN);
    }
}

impl Default for Volume {
    fn default() -> Self {
        Self(80)
    }
}

/// The main application state shared between threads.
/// This is read by the UI and written by the main thread (command processing).
#[derive(Debug, Clone)]
pub struct AppState {
    // File paths
    pub video_path: Option<PathBuf>,
    pub audio_model_path: Option<PathBuf>,

    // Backend selections
    pub video_backend: Backend,
    pub audio_backend: Backend,

    // Playback state
    pub playback_state: PlaybackState,

    // Time info (read-only for UI, updated by main thread)
    pub current_time_ns: u64,
    pub total_duration_ns: u64, // 0 if unknown

    // Volume (clamped to 0..100)
    pub volume: Volume,

    // Log panel
    pub log_lines: Vec<String>,
    pub show_logs: bool,
}

impl AppState {
    /// Returns true if a video file is loaded.
    pub fn is_video_loaded(&self) -> bool {
        self.video_path.is_some()
    }

    /// Returns true if playback can be started/resumed.
    pub fn can_play(&self) -> bool {
        self.is_video_loaded()
    }

    /// Returns true if playback is currently playing.
    pub fn is_playing(&self) -> bool {
        self.playback_state == PlaybackState::Playing && self.is_video_loaded()
    }

    /// Returns true if an audio model is loaded.
    pub fn has_audio_model(&self) -> bool {
        self.audio_model_path.is_some()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            video_path: None,
            audio_model_path: None,
            video_backend: Backend::default(),
            audio_backend: Backend::default(),
            playback_state: PlaybackState::default(),
            current_time_ns: 0,
            total_duration_ns: 0,
            volume: Volume::default(),
            log_lines: vec!["🚀 Media player ready".to_string()],
            show_logs: false,
        }
    }
}
