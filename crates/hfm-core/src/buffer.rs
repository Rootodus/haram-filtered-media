//! Window buffer for processed video and audio frames with PTS tracking.

use parking_lot::Mutex;
use std::collections::VecDeque;

/// Presentation timestamp in nanoseconds (monotonic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pts(pub u64);

/// A processed video frame (RGBA, full resolution).
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub pts: Pts,
    pub slot: usize,
    pub data: Vec<u8>,
}

/// A processed audio chunk (PCM, interleaved stereo f32).
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub pts: Pts,
    pub samples: Vec<f32>,
}

#[derive(Debug)]
enum BufferState {
    Empty,
    Seeking,
    Active,
}

pub struct MediaBuffer {
    video_queue: Mutex<VecDeque<VideoFrame>>,
    audio_queue: Mutex<VecDeque<AudioChunk>>,
    state: Mutex<BufferState>,
    // Last PTS for monotonicity checks (per stream)
    video_last_pts: Mutex<Option<Pts>>,
    audio_last_pts: Mutex<Option<Pts>>,
    // Known durations for fallback fill‑level (if PTS not available)
    video_frame_duration_secs: f32,
    audio_chunk_duration_secs: f32,
    capacity_secs: f32,
}

impl MediaBuffer {
    pub fn new(
        capacity_secs: f32,
        video_fps: f32,
        audio_sample_rate: u32,
        audio_window_samples: usize,
    ) -> Self {
        // We no longer need pre‑allocated capacity, but we can keep for reference.
        Self {
            video_queue: Mutex::new(VecDeque::new()),
            audio_queue: Mutex::new(VecDeque::new()),
            state: Mutex::new(BufferState::Empty),
            video_last_pts: Mutex::new(None),
            audio_last_pts: Mutex::new(None),
            video_frame_duration_secs: 1.0 / video_fps,
            audio_chunk_duration_secs: audio_window_samples as f32 / audio_sample_rate as f32,
            capacity_secs,
        }
    }

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

        let mut queue = self.video_queue.lock();
        // We don't have a fixed capacity, but we can optionally limit to prevent unbounded growth.
        // For now, we'll allow any size, but we could enforce a max length based on capacity.
        queue.push_back(frame);
        *self.video_last_pts.lock() = Some(pts);
        self.update_state_after_push();
        Ok(())
    }

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

        let mut queue = self.audio_queue.lock();
        queue.push_back(chunk);
        *self.audio_last_pts.lock() = Some(pts);
        self.update_state_after_push();
        Ok(())
    }

    pub fn pop_video(&self) -> Option<VideoFrame> {
        let mut queue = self.video_queue.lock();
        let frame = queue.pop_front()?;
        self.update_state_after_pop();
        Some(frame)
    }

    pub fn pop_audio(&self) -> Option<AudioChunk> {
        let mut queue = self.audio_queue.lock();
        let chunk = queue.pop_front()?;
        self.update_state_after_pop();
        Some(chunk)
    }

    fn update_state_after_push(&self) {
        let mut state = self.state.lock();
        if let BufferState::Empty | BufferState::Seeking = *state {
            *state = BufferState::Active;
        }
    }

    fn update_state_after_pop(&self) {
        let mut state = self.state.lock();
        if self.video_queue.lock().is_empty() && self.audio_queue.lock().is_empty() {
            if let BufferState::Seeking = *state {
                // remain Seeking
            } else {
                *state = BufferState::Empty;
            }
        }
    }

    fn is_seeking(&self) -> bool {
        matches!(*self.state.lock(), BufferState::Seeking)
    }

    /// Returns the current buffer fill level in seconds.
    /// Uses PTS of the oldest and newest frames if available; otherwise falls back to count × duration.
    /// Returns the *minimum* fill level across video and audio to ensure neither stream underruns.
    pub fn fill_level_secs(&self) -> f32 {
        let video_queue = self.video_queue.lock();
        let audio_queue = self.audio_queue.lock();

        // Helper: compute duration for a queue
        let queue_duration = |queue: &VecDeque<VideoFrame>, default_duration: f32| -> f32 {
            if queue.len() < 2 {
                queue.len() as f32 * default_duration
            } else {
                let oldest = queue.front().unwrap().pts;
                let newest = queue.back().unwrap().pts;
                (newest.0 - oldest.0) as f32 / 1_000_000_000.0
            }
        };

        // Compute durations (using a closure that works for both queues)
        let video_dur = if video_queue.is_empty() {
            0.0
        } else {
            queue_duration(&video_queue, self.video_frame_duration_secs)
        };

        let audio_dur = if audio_queue.is_empty() {
            // If audio is not used, we ignore it for throttling.
            // Returning f32::MAX effectively makes video the only decider.
            f32::MAX
        } else {
            // For audio, we need a separate helper. Since we don't have a generic type,
            // we'll inline the logic.
            if audio_queue.len() < 2 {
                audio_queue.len() as f32 * self.audio_chunk_duration_secs
            } else {
                let oldest = audio_queue.front().unwrap().pts;
                let newest = audio_queue.back().unwrap().pts;
                (newest.0 - oldest.0) as f32 / 1_000_000_000.0
            }
        };

        // Return the minimum – the bottleneck stream
        video_dur.min(audio_dur)
    }

    /// Flushes both queues and transitions to `Seeking` state.
    pub fn flush(&self) {
        self.video_queue.lock().clear();
        self.audio_queue.lock().clear();
        *self.video_last_pts.lock() = None;
        *self.audio_last_pts.lock() = None;
        *self.state.lock() = BufferState::Seeking;
    }

    /// Marks the seek as completed. Transitions to `Empty` or `Active` based on queue content.
    pub fn seek_completed(&self) {
        let mut state = self.state.lock();
        if let BufferState::Seeking = *state {
            if self.video_queue.lock().is_empty() && self.audio_queue.lock().is_empty() {
                *state = BufferState::Empty;
            } else {
                *state = BufferState::Active;
            }
        }
    }

    pub fn capacity_secs(&self) -> f32 {
        self.capacity_secs
    }

    pub fn video_len(&self) -> usize {
        self.video_queue.lock().len()
    }

    pub fn audio_len(&self) -> usize {
        self.audio_queue.lock().len()
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
            slot: 0,
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
    fn test_audio_construction() {
        let chunk = dummy_audio(1000);
        assert_eq!(chunk.pts.0, 1000);
        assert_eq!(chunk.samples.len(), 2048 * 2);
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
    fn test_fill_level_with_pts() {
        let buf = MediaBuffer::new(5.0, 30.0, 44100, 2048);
        let pts_start = 0;
        for i in 0..10 {
            let pts = pts_start + i * 33_333_333;
            assert!(buf.push_video(dummy_video(pts)).is_ok());
        }
        let fill = buf.fill_level_secs();
        // 10 frames at 30 fps = 0.333 sec (with PTS precision)
        assert!(fill >= 0.29 && fill <= 0.35);
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
