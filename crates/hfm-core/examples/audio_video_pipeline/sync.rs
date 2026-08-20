use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

/// High‑level playback state shared between threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    /// Not enough data to start or continue playback.
    Buffering,
    /// Normal playback.
    Playing,
    /// A seek is in progress; all buffers are being flushed.
    Seeking,
}

/// Shared audio‑driven media clock.
///
/// The audio output callback advances this clock as samples are played.
/// Video rendering uses it to delay frames until their PTS matches the
/// currently audible position.
pub struct AudioClock {
    /// Current audio playback position in nanoseconds.
    current_ns: AtomicU64,
    sample_rate: u32,
    /// Set to true when the first audio chunk is known.
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

    /// Called once when the first processed audio chunk is about to be
    /// delivered to the output ring buffer. This sets the clock base to the
    /// chunk’s PTS so that subsequent deltas produce absolute media time.
    pub fn set_base_pts(&self, pts_ns: u64) {
        self.current_ns.store(pts_ns, Ordering::Release);
        self.initialized.store(true, Ordering::Release);
    }

    /// Advance the clock by the number of interleaved frames (samples per
    /// channel) that were just played by CPAL.
    pub fn advance_by_frames(&self, frames: usize) {
        let delta_ns = (frames as u64 * 1_000_000_000) / self.sample_rate as u64;
        self.current_ns.fetch_add(delta_ns, Ordering::AcqRel);
    }

    /// Current media position in nanoseconds.
    pub fn now_ns(&self) -> u64 {
        self.current_ns.load(Ordering::Acquire)
    }

    /// Wait until the audio clock reaches `target_ns`.
    pub fn wait_until(&self, target_ns: u64) {
        loop {
            let now = self.now_ns();
            if now >= target_ns {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// Reset the clock, e.g. after a seek.
    pub fn reset(&self) {
        self.current_ns.store(0, Ordering::Release);
        self.initialized.store(false, Ordering::Release);
    }
}

/// Synchronisation controller shared between audio processing, video
/// rendering, and the demux pump.
pub struct AvSync {
    pub audio_clock: AudioClock,
    /// PTS of the last video frame that was actually displayed.
    last_video_pts_ns: AtomicU64,
    /// Maximum allowed lead of audio over video in nanoseconds.
    max_audio_lead_ns: u64,
    /// Set when video source has ended; audio may then run to completion.
    video_ended: AtomicBool,
    /// Shared playback state (Buffering, Playing, Seeking).
    state: RwLock<PlaybackState>,
    /// Flag to request the audio processor to flush its internal buffers.
    audio_flush_requested: AtomicBool,
}

impl AvSync {
    pub fn new(sample_rate: u32, max_audio_lead_ms: u64) -> Self {
        Self {
            audio_clock: AudioClock::new(sample_rate),
            last_video_pts_ns: AtomicU64::new(0),
            max_audio_lead_ns: max_audio_lead_ms * 1_000_000,
            video_ended: AtomicBool::new(false),
            state: RwLock::new(PlaybackState::Buffering),
            audio_flush_requested: AtomicBool::new(false),
        }
    }

    /// Set the current playback state.
    pub fn set_state(&self, new_state: PlaybackState) {
        *self.state.write().unwrap() = new_state;
    }

    /// Get the current playback state.
    pub fn get_state(&self) -> PlaybackState {
        *self.state.read().unwrap()
    }

    /// Call from the video rendering path immediately after a frame is drawn.
    pub fn report_video_pts(&self, pts_ns: u64) {
        self.last_video_pts_ns.store(pts_ns, Ordering::Release);
    }

    /// Mark that the video source has reached end‑of‑stream.
    pub fn set_video_ended(&self) {
        self.video_ended.store(true, Ordering::Release);
    }

    /// Reset the video-ended flag (used after seek).
    pub fn clear_video_ended(&self) {
        self.video_ended.store(false, Ordering::Release);
    }

    /// Called by the audio processing thread before pushing a processed
    /// chunk into the output ring buffer.
    ///
    /// If audio is ahead of video by more than `max_audio_lead_ns`, this
    /// method blocks until video catches up (or video has ended).
    pub fn gate_audio_output(&self, audio_pts_ns: u64) {
        // Do not gate during Buffering/Seeking.
        if self.get_state() != PlaybackState::Playing {
            return;
        }

        if self.video_ended.load(Ordering::Acquire) {
            return;
        }

        loop {
            let video_pts = self.last_video_pts_ns.load(Ordering::Acquire);
            if video_pts == 0 {
                break;
            }
            let lead = audio_pts_ns.saturating_sub(video_pts);
            if lead <= self.max_audio_lead_ns {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Wait until the audio clock reaches `target_ns`.
    pub fn wait_video(&self, target_ns: u64) {
        if !self.audio_clock.is_initialized() {
            return;
        }
        self.audio_clock.wait_until(target_ns);
    }

    /// Reset everything after a seek: audio clock, last video PTS, video-ended,
    /// and request audio processor to flush internal buffers.
    pub fn reset_after_seek(&self) {
        self.audio_clock.reset();
        self.last_video_pts_ns.store(0, Ordering::Release);
        self.clear_video_ended();
        self.audio_flush_requested.store(true, Ordering::Release);
        self.set_state(PlaybackState::Buffering);
    }

    /// Check whether the audio processor should flush its internal buffers.
    pub fn is_audio_flush_requested(&self) -> bool {
        self.audio_flush_requested.load(Ordering::Acquire)
    }

    /// Clear the audio flush flag (call after flushing).
    pub fn clear_audio_flush_requested(&self) {
        self.audio_flush_requested.store(false, Ordering::Release);
    }

    /// Accessor for the audio clock (for CPAL callback).
    pub fn audio_clock(&self) -> &AudioClock {
        &self.audio_clock
    }
}
