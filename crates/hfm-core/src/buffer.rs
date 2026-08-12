//! Window buffer for processed video and audio frames with PTS tracking.
//!
//! This buffer sits between the ML processing threads and the render/output threads.
//! It stores processed frames and audio chunks with their original presentation timestamps,
//! and provides fill‑level measurement for throttling.
//!
//! # State Machine
//!
//! The buffer uses an enum state to enforce invariants:
//! - `Empty`: No frames in either queue.
//! - `Seeking`: A seek is in progress; PTS monotonicity checks are relaxed.
//! - `Active`: Normal playback; at least one queue has data.

use crossbeam::queue::ArrayQueue;
use parking_lot::Mutex;

/// Presentation timestamp in nanoseconds (monotonic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pts(pub u64);

/// A processed video frame (RGBA, full resolution).
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub pts: Pts,
    pub data: Vec<u8>, // length = width * height * 4
}

/// A processed audio chunk (PCM, interleaved stereo f32).
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub pts: Pts,
    pub samples: Vec<f32>, // length = window_samples * channels
}

/// State of the buffer.
#[derive(Debug)]
enum BufferState {
    Empty,
    Seeking,
    Active,
}

/// The window buffer.
pub struct MediaBuffer {
    video_queue: ArrayQueue<VideoFrame>,
    audio_queue: ArrayQueue<AudioChunk>,
    state: Mutex<BufferState>,
    // Last PTS for monotonicity checks (per stream)
    video_last_pts: Mutex<Option<Pts>>,
    audio_last_pts: Mutex<Option<Pts>>,
    // Known durations for fill‑level calculation
    video_frame_duration_secs: f32, // 1 / fps
    audio_chunk_duration_secs: f32, // window_samples / sample_rate
    capacity_secs: f32,
}

impl MediaBuffer {
    /// Creates a new buffer.
    pub fn new(
        capacity_secs: f32,
        video_fps: f32,
        audio_sample_rate: u32,
        audio_window_samples: usize,
    ) -> Self {
        let video_capacity = (capacity_secs * video_fps).ceil() as usize + 2;
        let audio_chunk_duration_secs = audio_window_samples as f32 / audio_sample_rate as f32;
        let audio_capacity = (capacity_secs / audio_chunk_duration_secs).ceil() as usize + 2;

        Self {
            video_queue: ArrayQueue::new(video_capacity),
            audio_queue: ArrayQueue::new(audio_capacity),
            state: Mutex::new(BufferState::Empty),
            video_last_pts: Mutex::new(None),
            audio_last_pts: Mutex::new(None),
            video_frame_duration_secs: 1.0 / video_fps,
            audio_chunk_duration_secs,
            capacity_secs,
        }
    }

    /// Pushes a video frame. Returns `Err(frame)` if the queue is full.
    pub fn push_video(&self, frame: VideoFrame) -> Result<(), VideoFrame> {
        let pts = frame.pts;
        // Monotonicity check unless seeking
        if !self.is_seeking() {
            if let Some(last_pts) = *self.video_last_pts.lock() {
                debug_assert!(
                    pts >= last_pts,
                    "Video PTS went backwards: {:?} < {:?}",
                    pts,
                    last_pts
                );
            }
        }

        match self.video_queue.push(frame) {
            Ok(()) => {
                *self.video_last_pts.lock() = Some(pts);
                self.update_state_after_push();
                Ok(())
            }
            Err(frame) => Err(frame),
        }
    }

    /// Pushes an audio chunk.
    pub fn push_audio(&self, chunk: AudioChunk) -> Result<(), AudioChunk> {
        let pts = chunk.pts;
        if !self.is_seeking() {
            if let Some(last_pts) = *self.audio_last_pts.lock() {
                debug_assert!(
                    pts >= last_pts,
                    "Audio PTS went backwards: {:?} < {:?}",
                    pts,
                    last_pts
                );
            }
        }

        match self.audio_queue.push(chunk) {
            Ok(()) => {
                *self.audio_last_pts.lock() = Some(pts);
                self.update_state_after_push();
                Ok(())
            }
            Err(chunk) => Err(chunk),
        }
    }

    /// Pops a video frame.
    pub fn pop_video(&self) -> Option<VideoFrame> {
        let frame = self.video_queue.pop()?;
        self.update_state_after_pop();
        Some(frame)
    }

    /// Pops an audio chunk.
    pub fn pop_audio(&self) -> Option<AudioChunk> {
        let chunk = self.audio_queue.pop()?;
        self.update_state_after_pop();
        Some(chunk)
    }

    /// Updates state after a push: if we were Empty or Seeking, become Active.
    fn update_state_after_push(&self) {
        let mut state = self.state.lock();
        if let BufferState::Empty | BufferState::Seeking = *state {
            *state = BufferState::Active;
        }
    }

    /// Updates state after a pop: if both queues are empty, become Empty (or keep Seeking if we were).
    fn update_state_after_pop(&self) {
        let mut state = self.state.lock();
        if self.video_queue.is_empty() && self.audio_queue.is_empty() {
            // If we were seeking, remain Seeking (waiting for new data)
            if let BufferState::Seeking = *state {
                // remain Seeking
            } else {
                *state = BufferState::Empty;
            }
        }
        // If we still have data, we are Active (already, so no change).
    }

    /// Returns `true` if the buffer is in seeking state.
    fn is_seeking(&self) -> bool {
        matches!(*self.state.lock(), BufferState::Seeking)
    }

    /// Returns the current buffer fill level in seconds (estimated from counts and known durations).
    pub fn fill_level_secs(&self) -> f32 {
        let video_dur = self.video_queue.len() as f32 * self.video_frame_duration_secs;
        let audio_dur = self.audio_queue.len() as f32 * self.audio_chunk_duration_secs;
        // We take the max because video and audio may be slightly out of sync.
        // But both should cover roughly the same time window.
        video_dur.max(audio_dur)
    }

    /// Flushes both queues and transitions to `Seeking` state.
    pub fn flush(&self) {
        while self.video_queue.pop().is_some() {}
        while self.audio_queue.pop().is_some() {}
        *self.video_last_pts.lock() = None;
        *self.audio_last_pts.lock() = None;
        *self.state.lock() = BufferState::Seeking;
    }

    /// Marks the seek as completed. Transitions to `Empty` or `Active` based on queue content.
    pub fn seek_completed(&self) {
        let mut state = self.state.lock();
        if let BufferState::Seeking = *state {
            if self.video_queue.is_empty() && self.audio_queue.is_empty() {
                *state = BufferState::Empty;
            } else {
                *state = BufferState::Active;
            }
        }
    }

    /// Returns the capacity in seconds.
    pub fn capacity_secs(&self) -> f32 {
        self.capacity_secs
    }

    pub fn video_len(&self) -> usize {
        self.video_queue.len()
    }

    pub fn audio_len(&self) -> usize {
        self.audio_queue.len()
    }
}

/// Convenience constructor.
pub fn default_buffer() -> MediaBuffer {
    MediaBuffer::new(5.0, 30.0, 44100, 2048)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_video(pts: u64) -> VideoFrame {
        VideoFrame {
            pts: Pts(pts),
            data: vec![0u8; 4 * 960 * 540],
        }
    }

    fn dummy_audio(pts: u64) -> AudioChunk {
        AudioChunk {
            pts: Pts(pts),
            samples: vec![0.0f32; 2048 * 2],
        }
    }

    #[test]
    fn test_push_pop() {
        let buf = MediaBuffer::new(1.0, 30.0, 44100, 2048);
        let frame = dummy_video(1000);
        assert!(buf.push_video(frame).is_ok());
        assert_eq!(buf.video_len(), 1);
        let popped = buf.pop_video().unwrap();
        assert_eq!(popped.pts.0, 1000);
        assert!(buf.pop_video().is_none());
        assert!(matches!(*buf.state.lock(), BufferState::Empty));
    }

    #[test]
    fn test_audio_push_pop() {
        let buf = MediaBuffer::new(1.0, 30.0, 44100, 2048);
        let chunk = dummy_audio(1000);
        assert!(buf.push_audio(chunk).is_ok());
        assert_eq!(buf.audio_len(), 1);
        let popped = buf.pop_audio().unwrap();
        assert_eq!(popped.pts.0, 1000);
        assert!(buf.pop_audio().is_none());
        assert!(matches!(*buf.state.lock(), BufferState::Empty));
    }

    #[test]
    fn test_fill_level() {
        let buf = MediaBuffer::new(5.0, 30.0, 44100, 2048);
        for i in 0..10 {
            let pts = i * 33_333_333;
            assert!(buf.push_video(dummy_video(pts)).is_ok());
        }
        let fill = buf.fill_level_secs();
        // 10 frames at 30 fps = 0.333 sec
        assert!(fill >= 0.32 && fill <= 0.35);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Video PTS went backwards")]
    fn test_monotonic_violation() {
        let buf = MediaBuffer::new(1.0, 30.0, 44100, 2048);
        buf.push_video(dummy_video(1000)).unwrap();
        buf.push_video(dummy_video(500)).unwrap();
    }

    #[test]
    fn test_seek() {
        let buf = MediaBuffer::new(1.0, 30.0, 44100, 2048);
        buf.push_video(dummy_video(1000)).unwrap();
        buf.flush();
        assert_eq!(buf.video_len(), 0);
        assert!(matches!(*buf.state.lock(), BufferState::Seeking));

        buf.push_video(dummy_video(500)).unwrap();
        assert_eq!(buf.video_len(), 1);
        buf.seek_completed();
        assert!(matches!(*buf.state.lock(), BufferState::Active));

        assert!(buf.push_video(dummy_video(600)).is_ok());
    }
}
