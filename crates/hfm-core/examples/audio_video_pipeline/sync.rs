//! Minimal shared primitives for audio/video coordination.
//!
//! This file must stay free of GStreamer, ONNX, CPAL, and wgpu dependencies.
//! It contains exactly three things:
//! - `AudioClock`        : current playback time, advanced by the audio output callback
//! - `SeekGeneration`    : atomic counter used to discard stale work after a seek
//! - `BufferingFlag`     : atomic flag used by the renderer to show black during underrun
//!
//! No complex state machine. No cross-thread blocking.

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
    pub fn set_base_pts(&self, pts_ns: u64) {
        self.current_ns.store(pts_ns, Ordering::Release);
        self.initialized.store(true, Ordering::Release);
    }

    /// Advance the clock by `frames` frames played by the audio device.
    ///
    /// Does nothing while the clock is uninitialized. This prevents
    /// pre-seek audio from advancing a reset media clock.
    pub fn advance_by_frames(&self, frames: usize) {
        if !self.initialized.load(Ordering::Acquire) {
            return;
        }

        let delta_ns = (frames as u64 * 1_000_000_000) / self.sample_rate as u64;
        self.current_ns.fetch_add(delta_ns, Ordering::AcqRel);
    }

    /// Current media position in nanoseconds.
    pub fn now_ns(&self) -> u64 {
        self.current_ns.load(Ordering::Acquire)
    }

    /// True after the clock has been initialized with an actual PTS.
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// Reset the clock after a seek.
    pub fn reset(&self) {
        self.current_ns.store(0, Ordering::Release);
        self.initialized.store(false, Ordering::Release);
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
        self.generation.load(Ordering::Acquire)
    }

    /// Increment the generation. Called on each seek.
    pub fn increment(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
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

    pub fn set(&self, value: bool) {
        self.buffering.store(value, Ordering::Release);
    }

    pub fn is_buffering(&self) -> bool {
        self.buffering.load(Ordering::Acquire)
    }
}
