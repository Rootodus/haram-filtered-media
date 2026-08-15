//! Pipeline orchestrating the ingest, ML processing, and buffering.
//! This module is source‑agnostic; it works with any implementor of `FrameSource`.

use crate::buffer::{MediaBuffer, Pts, VideoFrame};
use crate::filter::VideoFilter;
use crate::memory::{PackedIndex, STATE_INGESTED, STATE_ML_COMMITTED, SlotPool};
use crate::ml::PeopleSegFilter;
use crossbeam::queue::ArrayQueue;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Constants for the video resolution (must match the model's expected input size).
pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 540;
pub const VIDEO_SLOT_SIZE: usize = (WIDTH * HEIGHT * 4) as usize;
pub const N_V: usize = 128;

/// A source of video frames.
pub trait FrameSource: Send + Sync {
    /// Pull the next frame, returning RGBA data and its PTS in nanoseconds.
    fn pull_frame(&mut self) -> Option<(Vec<u8>, u64)>;

    /// Seek by a delta (positive = forward, negative = backward) in nanoseconds.
    /// Returns `Ok(())` on success, or an error string if seeking is not supported or fails.
    fn seek(&mut self, delta_ns: i64) -> Result<(), String>;
}

/// The video pipeline. Owns the source, the ML model, the memory pool, and the buffer.
pub struct VideoPipeline {
    source_mutex: Arc<Mutex<Box<dyn FrameSource>>>,
    model: Arc<PeopleSegFilter>,
    pool: Arc<SlotPool<VIDEO_SLOT_SIZE>>,
    buffer: Arc<MediaBuffer>,
    ingest_queue: Arc<ArrayQueue<PackedIndex>>,
    seek_gen: Arc<AtomicU64>,
    _ingest_handle: Option<std::thread::JoinHandle<()>>,
    _ml_handle: Option<std::thread::JoinHandle<()>>,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl VideoPipeline {
    /// Creates a new pipeline with the given source and model.
    /// The pipeline is not started until `start()` is called.
    pub fn new(source: Box<dyn FrameSource>, model: PeopleSegFilter) -> Self {
        let pool = Arc::new(SlotPool::<VIDEO_SLOT_SIZE>::new(N_V));
        let buffer = Arc::new(MediaBuffer::new(5.0, 30.0, 44100, 2048));
        let ingest_queue = Arc::new(ArrayQueue::<PackedIndex>::new(N_V));
        let seek_gen = Arc::new(AtomicU64::new(0));
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

        VideoPipeline {
            source_mutex: Arc::new(Mutex::new(source)),
            model: Arc::new(model),
            pool,
            buffer,
            ingest_queue,
            seek_gen,
            _ingest_handle: None,
            _ml_handle: None,
            running,
        }
    }

    /// Starts the ingest and ML threads.
    pub fn start(&mut self) {
        let source_mutex = self.source_mutex.clone();
        let pool = self.pool.clone();
        let ingest_queue = self.ingest_queue.clone();
        let buffer = self.buffer.clone();
        let model = self.model.clone();
        let seek_gen = self.seek_gen.clone();
        let running = self.running.clone();

        // Ingest thread
        let ingest_source = source_mutex.clone();
        let ingest_pool = pool.clone();
        let ingest_queue_clone = ingest_queue.clone();
        let ingest_buffer = buffer.clone();
        let ingest_seek_gen = seek_gen.clone();
        let ingest_running = running.clone();
        let ingest_handle = std::thread::spawn(move || {
            Self::ingest_loop(
                ingest_source,
                ingest_pool,
                ingest_queue_clone,
                ingest_buffer,
                ingest_seek_gen,
                ingest_running,
            );
        });

        // ML thread
        let ml_pool = pool.clone();
        let ml_ingest_queue = ingest_queue.clone();
        let ml_buffer = buffer.clone();
        let ml_model = model.clone();
        let ml_seek_gen = seek_gen.clone();
        let ml_running = running.clone();
        let ml_handle = std::thread::spawn(move || {
            Self::ml_loop(
                ml_pool,
                ml_ingest_queue,
                ml_buffer,
                ml_model,
                ml_seek_gen,
                ml_running,
            );
        });

        self._ingest_handle = Some(ingest_handle);
        self._ml_handle = Some(ml_handle);
    }

    /// Pops a processed video frame from the buffer, if available.
    pub fn pop_processed_frame(&self) -> Option<VideoFrame> {
        self.buffer.pop_video()
    }

    /// Seeks by a delta (positive forward, negative backward) in nanoseconds.
    /// This flushes the buffer, increments the generation, and forwards the seek to the source.
    pub fn seek(&self, delta_ns: i64) -> Result<(), String> {
        self.seek_gen.fetch_add(1, Ordering::Release);
        self.buffer.flush();
        self.source_mutex.lock().seek(delta_ns)
    }

    // Private ingest loop
    fn ingest_loop(
        source_mutex: Arc<Mutex<Box<dyn FrameSource>>>,
        pool: Arc<SlotPool<VIDEO_SLOT_SIZE>>,
        ingest_queue: Arc<ArrayQueue<PackedIndex>>,
        _buffer: Arc<MediaBuffer>,
        seek_gen: Arc<AtomicU64>,
        running: Arc<std::sync::atomic::AtomicBool>,
    ) {
        while running.load(Ordering::Acquire) {
            // Pull a frame from the source
            let (rgba, pts_ns) = match source_mutex.lock().pull_frame() {
                Some(data) => data,
                None => {
                    // End of stream or error – break the loop
                    break;
                }
            };

            // Claim a slot
            if let Some(packed) = pool.try_claim() {
                // Write data
                pool.with_payload_mut(packed, |payload| {
                    payload.copy_from_slice(&rgba);
                });
                // Store PTS and current generation
                pool.set_pts_ns(packed, pts_ns);
                let current_gen = seek_gen.load(Ordering::Acquire);
                pool.set_seek_gen(packed, current_gen);

                // Push to ingest queue
                while let Err(_) = ingest_queue.push(packed) {
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
            } else {
                // No slot available – wait
                std::thread::sleep(std::time::Duration::from_micros(100));
            }
        }
    }

    // Private ML loop
    fn ml_loop(
        pool: Arc<SlotPool<VIDEO_SLOT_SIZE>>,
        ingest_queue: Arc<ArrayQueue<PackedIndex>>,
        buffer: Arc<MediaBuffer>,
        model: Arc<PeopleSegFilter>,
        seek_gen: Arc<AtomicU64>,
        running: Arc<std::sync::atomic::AtomicBool>,
    ) {
        while running.load(Ordering::Acquire) {
            if let Some(packed) = ingest_queue.pop() {
                // Check generation before inference
                let slot_gen = pool.get_seek_gen(packed);
                let current_gen = seek_gen.load(Ordering::Acquire);
                if slot_gen != current_gen {
                    pool.discard_slot(packed);
                    continue;
                }

                // Run inference
                let _result = pool
                    .with_payload_mut(packed, |payload| model.filter_frame(payload, WIDTH, HEIGHT));

                // Check generation after inference
                let current_gen_after = seek_gen.load(Ordering::Acquire);
                if slot_gen != current_gen_after {
                    pool.discard_slot(packed);
                    continue;
                }

                // Transition state and push to buffer
                pool.transition_state(packed, STATE_INGESTED, STATE_ML_COMMITTED)
                    .expect("State transition failed");

                let pts_ns = pool.get_pts_ns(packed);
                let pts = Pts(pts_ns);
                let data = pool.with_payload_mut(packed, |p| p.to_vec());
                let frame = VideoFrame {
                    pts,
                    slot: packed,
                    data,
                    seek_gen: slot_gen,
                };

                if let Err(_) = buffer.push_video(frame) {
                    // Buffer rejected (flush happened) – discard the slot
                    pool.discard_slot(packed);
                }
            } else {
                std::thread::sleep(std::time::Duration::from_micros(100));
            }
        }
    }
}
