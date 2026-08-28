//! Shared state between UI and pipeline.

use hfm_core::coordination::PlaybackState;
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

/// Volume value clamped to 0..100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume(u8);

impl Volume {
    pub const MIN: u8 = 0;
    pub const MAX: u8 = 100;

    pub fn new(value: u8) -> Self {
        Self(value.clamp(Self::MIN, Self::MAX))
    }

    pub fn get(&self) -> u8 {
        self.0
    }

    pub fn step_up(&mut self, step: u8) {
        self.0 = (self.0 + step).min(Self::MAX);
    }

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
#[derive(Debug, Clone)]
pub struct AppState {
    // Video file
    pub video_path: Option<PathBuf>,

    // Backend selections
    pub video_backend: Backend,
    pub audio_backend: Backend,

    // Feature toggles
    pub video_filter_enabled: bool,
    pub audio_processing_enabled: bool,

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

    // UI mode
    pub mode: AppMode,

    // Bottom panel
    pub bottom_panel_height: f32,

    // Loading
    pub is_loading: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Setup,
    Playback,
}

impl Default for AppMode {
    fn default() -> Self {
        Self::Setup
    }
}

impl AppState {
    /// Returns true if a video file is loaded.
    pub fn is_video_loaded(&self) -> bool {
        self.video_path.is_some()
    }

    /// Returns true if playback is currently playing.
    pub fn is_playing(&self) -> bool {
        self.playback_state == PlaybackState::Playing && self.is_video_loaded()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            video_path: None,
            video_backend: Backend::default(),
            audio_backend: Backend::default(),
            video_filter_enabled: false,
            audio_processing_enabled: false,
            playback_state: PlaybackState::Paused,
            current_time_ns: 0,
            total_duration_ns: 0,
            volume: Volume::default(),
            log_lines: vec!["Media player ready".to_string()],
            show_logs: false,
            mode: AppMode::default(),
            bottom_panel_height: 0.0,
            is_loading: false,
        }
    }
}
