//! Window buffer for processed video and audio frames with PTS tracking.
//! Designed for single‑producer (ML thread) and single‑consumer (render thread).

use crossbeam::queue::ArrayQueue;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

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

/// The window buffer. Holds video and audio separately but enforces common invariants.
pub struct MediaBuffer {
    video_queue: ArrayQueue<VideoFrame>,
    audio_queue: ArrayQueue<AudioChunk>,
    // For fill‑level measurement (PTS of oldest and newest items)
    video_oldest_pts: Mutex<Option<Pts>>,
    video_newest_pts: Mutex<Option<Pts>>,
    audio_oldest_pts: Mutex<Option<Pts>>,
    audio_newest_pts: Mutex<Option<Pts>>,
    // Seek flag: when true, monotonicity checks are relaxed.
    seek_pending: Arc<Mutex<bool>>,
    // Capacity in seconds (for reference)
    capacity_secs: f32,
}

impl MediaBuffer {
    /// Creates a new buffer with time‑based capacity.
    /// `video_fps` – expected frame rate for fill‑level estimation (though we use PTS).
    /// `audio_sample_rate` – sample rate in Hz.
    /// `audio_window_samples` – number of samples per audio chunk.
    pub fn new(
        capacity_secs: f32,
        video_fps: f32,
        audio_sample_rate: u32,
        audio_window_samples: usize,
    ) -> Self {
        // Compute max number of video frames (round up)
        let video_capacity = (capacity_secs * video_fps).ceil() as usize + 2; // +2 for safety
        // Compute max audio chunks (assuming chunks are non‑overlapping)
        let audio_chunk_duration_secs = audio_window_samples as f32 / audio_sample_rate as f32;
        let audio_capacity = (capacity_secs / audio_chunk_duration_secs).ceil() as usize + 2;

        Self {
            video_queue: ArrayQueue::new(video_capacity),
            audio_queue: ArrayQueue::new(audio_capacity),
            video_oldest_pts: Mutex::new(None),
            video_newest_pts: Mutex::new(None),
            audio_oldest_pts: Mutex::new(None),
            audio_newest_pts: Mutex::new(None),
            seek_pending: Arc::new(Mutex::new(false)),
            capacity_secs,
        }
    }

    /// Pushes a video frame. Returns `Err(frame)` if the queue is full.
    /// In debug builds, checks that PTS is monotonic (unless seek pending).
    pub fn push_video(&self, frame: VideoFrame) -> Result<(), VideoFrame> {
        let pts = frame.pts;
        // Monotonicity check (unless seek pending)
        if !self.is_seek_pending() {
            if let Some(last_pts) = *self.video_newest_pts.lock() {
                debug_assert!(
                    pts >= last_pts,
                    "Video PTS went backwards: {:?} < {:?}",
                    pts,
                    last_pts
                );
            }
        }
        // Attempt push
        match self.video_queue.push(frame) {
            Ok(()) => {
                // Update oldest/newest PTS
                let mut oldest = self.video_oldest_pts.lock();
                let mut newest = self.video_newest_pts.lock();
                if oldest.is_none() {
                    *oldest = Some(pts);
                }
                *newest = Some(pts);
                Ok(())
            }
            Err(frame) => Err(frame),
        }
    }

    /// Pushes an audio chunk. Similar to `push_video`.
    pub fn push_audio(&self, chunk: AudioChunk) -> Result<(), AudioChunk> {
        let pts = chunk.pts;
        if !self.is_seek_pending() {
            if let Some(last_pts) = *self.audio_newest_pts.lock() {
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
                let mut oldest = self.audio_oldest_pts.lock();
                let mut newest = self.audio_newest_pts.lock();
                if oldest.is_none() {
                    *oldest = Some(pts);
                }
                *newest = Some(pts);
                Ok(())
            }
            Err(chunk) => Err(chunk),
        }
    }

    /// Pops a video frame (FIFO). Returns `None` if empty.
    pub fn pop_video(&self) -> Option<VideoFrame> {
        let frame = self.video_queue.pop()?;
        // Update oldest PTS after pop
        let mut oldest = self.video_oldest_pts.lock();
        if let Some(front) = self.video_queue.front() {
            *oldest = Some(front.pts);
        } else {
            *oldest = None;
        }
        Some(frame)
    }

    /// Pops an audio chunk.
    pub fn pop_audio(&self) -> Option<AudioChunk> {
        let chunk = self.audio_queue.pop()?;
        let mut oldest = self.audio_oldest_pts.lock();
        if let Some(front) = self.audio_queue.front() {
            *oldest = Some(front.pts);
        } else {
            *oldest = None;
        }
        Some(chunk)
    }

    /// Returns the current buffer fill level in seconds (video side, but both should match).
    pub fn fill_level_secs(&self) -> f32 {
        let oldest = *self.video_oldest_pts.lock();
        let newest = *self.video_newest_pts.lock();
        match (oldest, newest) {
            (Some(o), Some(n)) => (n.0 - o.0) as f32 / 1_000_000_000.0,
            _ => 0.0,
        }
    }

    /// Flushes both queues and resets PTS tracking.
    pub fn flush(&self) {
        // Clear queues (drain)
        while self.video_queue.pop().is_some() {}
        while self.audio_queue.pop().is_some() {}
        *self.video_oldest_pts.lock() = None;
        *self.video_newest_pts.lock() = None;
        *self.audio_oldest_pts.lock() = None;
        *self.audio_newest_pts.lock() = None;
        // Mark seek pending so monotonicity checks are relaxed for next pushes
        self.set_seek_pending(true);
    }

    /// Sets the seek pending flag. When true, PTS monotonicity checks are disabled.
    pub fn set_seek_pending(&self, pending: bool) {
        *self.seek_pending.lock() = pending;
    }

    fn is_seek_pending(&self) -> bool {
        *self.seek_pending.lock()
    }

    /// The capacity in seconds (read‑only).
    pub fn capacity_secs(&self) -> f32 {
        self.capacity_secs
    }

    /// Returns the number of video frames currently in the buffer.
    pub fn video_len(&self) -> usize {
        self.video_queue.len()
    }

    /// Returns the number of audio chunks currently in the buffer.
    pub fn audio_len(&self) -> usize {
        self.audio_queue.len()
    }
}

/// Convenience constructor for typical 30 fps video and 44.1 kHz audio with 2048‑sample chunks.
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
    }

    #[test]
    fn test_fill_level() {
        let buf = MediaBuffer::new(5.0, 30.0, 44100, 2048);
        let pts_start = 0;
        for i in 0..10 {
            let pts = pts_start + i * 33_333_333; // 30 fps
            assert!(buf.push_video(dummy_video(pts)).is_ok());
        }
        let fill = buf.fill_level_secs();
        assert!(fill >= 0.29 && fill <= 0.35);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Video PTS went backwards")]
    fn test_monotonic_violation() {
        let buf = MediaBuffer::new(1.0, 30.0, 44100, 2048);
        buf.push_video(dummy_video(1000)).unwrap();
        // This should panic because PTS went backwards
        buf.push_video(dummy_video(500)).unwrap();
    }

    #[test]
    fn test_seek() {
        let buf = MediaBuffer::new(1.0, 30.0, 44100, 2048);
        buf.push_video(dummy_video(1000)).unwrap();
        buf.flush();
        assert_eq!(buf.video_len(), 0);
        // After flush, monotonicity is relaxed; this should not panic
        buf.push_video(dummy_video(500)).unwrap();
        assert_eq!(buf.video_len(), 1);
    }
}
