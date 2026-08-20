//! Minimal shared primitives for audio/video coordination.
//!
//! This file must stay free of GStreamer, ONNX, CPAL, and wgpu dependencies.
//! It contains exactly three things:
//! - `AudioClock`        : current playback time, advanced by the audio output callback
//! - `SeekGeneration`    : atomic counter used to discard stale work after a seek
//! - `BufferingFlag`     : atomic flag used by the renderer to show black during underrun
//!
//! No complex state machine. No cross-thread blocking.
//!
//! NOTE: This is an interface-first skeleton. Constructors and trivial getters
//! are implemented so `main.rs` can compile. All operational methods are stubs.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Shared audio-driven media clock.
pub struct AudioClock {
    current_ns: AtomicU64,
    sample_rate: u32,
    initialized: AtomicBool,
}

impl AudioClock {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            current_ns: AtomicU64::new(0),
            sample_rate,
            initialized: AtomicBool::new(false),
        }
    }

    /// Called once when the first processed audio chunk is ready.
    pub fn set_base_pts(&self, _pts_ns: u64) {
        todo!("AudioClock::set_base_pts")
    }

    /// Advance the clock by `frames` frames played by the audio device.
    pub fn advance_by_frames(&self, _frames: usize) {
        todo!("AudioClock::advance_by_frames")
    }

    /// Current media position in nanoseconds.
    pub fn now_ns(&self) -> u64 {
        self.current_ns.load(Ordering::Relaxed)
    }

    /// True after the clock has been initialized with an actual PTS.
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }

    /// Reset the clock after a seek.
    pub fn reset(&self) {
        todo!("AudioClock::reset")
    }
}

/// Atomic seek generation counter.
pub struct SeekGeneration {
    generation: AtomicU64,
}

impl SeekGeneration {
    pub fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
        }
    }

    /// Current generation value.
    pub fn current(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Increment the generation. Called on each seek.
    pub fn increment(&self) -> u64 {
        todo!("SeekGeneration::increment")
    }
}

/// Atomic buffering flag.
pub struct BufferingFlag {
    buffering: AtomicBool,
}

impl BufferingFlag {
    pub fn new(initial: bool) -> Self {
        Self {
            buffering: AtomicBool::new(initial),
        }
    }

    pub fn set(&self, _value: bool) {
        todo!("BufferingFlag::set")
    }

    pub fn is_buffering(&self) -> bool {
        self.buffering.load(Ordering::Relaxed)
    }
}

/// Plain container for the shared primitives.
pub struct AvSync {
    pub audio_clock: AudioClock,
    pub generation: SeekGeneration,
    pub buffering: BufferingFlag,
}

impl AvSync {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            audio_clock: AudioClock::new(sample_rate),
            generation: SeekGeneration::new(),
            buffering: BufferingFlag::new(true), // start in buffering state
        }
    }
}
