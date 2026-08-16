//! Pipeline orchestrating the ingest, ML processing, and buffering.
//! This module is source‑agnostic; it works with any implementor of `FrameSource`.

use crate::buffer::{MediaBuffer, Pts, VideoFrame};
use crate::filter::VideoFilter;
use crate::memory::{PackedIndex, STATE_INGESTED, STATE_ML_COMMITTED, SlotPool};
use crate::ml::PeopleSegFilter;
use crossbeam::queue::ArrayQueue;
use crossbeam_channel::{Receiver, Sender, bounded};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Constants for the video resolution (must match the model's expected input size).
pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 540;
pub const VIDEO_SLOT_SIZE: usize = (WIDTH * HEIGHT * 4) as usize;
pub const N_V: usize = 128;

/// A source of video frames.
pub trait FrameSource: Send + Sync {
    fn pull_frame(&mut self) -> Option<(Vec<u8>, u64)>;
    fn seek(&mut self, delta_ns: i64) -> Result<(), String>;
}

/// The video pipeline. Owns the ML model, the memory pool, the buffer, and the ingest thread.
pub struct VideoPipeline {
    source: Option<Box<dyn FrameSource>>, // moved into ingest thread on start
    model: Arc<PeopleSegFilter>,
    pool: Arc<SlotPool<VIDEO_SLOT_SIZE>>,
    buffer: Arc<MediaBuffer>,
    ingest_queue: Arc<ArrayQueue<PackedIndex>>,
    seek_gen: Arc<AtomicU64>,
    seek_cmd_tx: Sender<i64>,
    _seek_cmd_rx: Receiver<i64>,
    _ingest_handle: Option<std::thread::JoinHandle<()>>,
    _ml_handle: Option<std::thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
}

impl VideoPipeline {
    pub fn new(source: Box<dyn FrameSource>, model: PeopleSegFilter) -> Self {
        let pool = Arc::new(SlotPool::<VIDEO_SLOT_SIZE>::new(N_V));
        let buffer = Arc::new(MediaBuffer::new(5.0, 30.0, 44100, 2048));
        let ingest_queue = Arc::new(ArrayQueue::<PackedIndex>::new(N_V));
        let seek_gen = Arc::new(AtomicU64::new(0));
        let running = Arc::new(AtomicBool::new(true));
        let (tx, rx) = bounded(1);

        VideoPipeline {
            source: Some(source),
            model: Arc::new(model),
            pool,
            buffer,
            ingest_queue,
            seek_gen,
            seek_cmd_tx: tx,
            _seek_cmd_rx: rx,
            _ingest_handle: None,
            _ml_handle: None,
            running,
        }
    }

    pub fn start(&mut self) {
        let source = self.source.take().expect("Pipeline already started");
        let pool = self.pool.clone();
        let ingest_queue = self.ingest_queue.clone();
        let buffer = self.buffer.clone();
        let model = self.model.clone();
        let seek_gen = self.seek_gen.clone();
        let running = self.running.clone();
        let seek_cmd_rx = self._seek_cmd_rx.clone();

        // Ingest thread – now owns the source directly
        let ingest_pool = pool.clone();
        let ingest_queue_clone = ingest_queue.clone();
        let ingest_buffer = buffer.clone();
        let ingest_seek_gen = seek_gen.clone();
        let ingest_running = running.clone();
        let ingest_handle = std::thread::spawn(move || {
            Self::ingest_loop(
                source,
                ingest_pool,
                ingest_queue_clone,
                ingest_buffer,
                ingest_seek_gen,
                seek_cmd_rx,
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

    pub fn pop_processed_frame(&self) -> Option<VideoFrame> {
        self.buffer.pop_video()
    }

    /// Send a seek command (delta in nanoseconds) to the ingest thread.
    pub fn seek(&self, delta_ns: i64) -> Result<(), String> {
        // We ignore the result; if the channel is full, the seek is already pending.
        let _ = self.seek_cmd_tx.try_send(delta_ns);
        Ok(())
    }

    // Private ingest loop – now receives the source as a mutable parameter
    fn ingest_loop(
        mut source: Box<dyn FrameSource>,
        pool: Arc<SlotPool<VIDEO_SLOT_SIZE>>,
        ingest_queue: Arc<ArrayQueue<PackedIndex>>,
        buffer: Arc<MediaBuffer>,
        seek_gen: Arc<AtomicU64>,
        seek_cmd_rx: Receiver<i64>,
        running: Arc<AtomicBool>,
    ) {
        let mut seeking = false;

        while running.load(Ordering::Acquire) {
            // Check for seek commands
            if let Ok(delta_ns) = seek_cmd_rx.try_recv() {
                if !seeking {
                    seeking = true;
                    seek_gen.fetch_add(1, Ordering::Release);
                    buffer.flush();
                    while ingest_queue.pop().is_some() {}
                    match source.seek(delta_ns) {
                        Ok(()) => {
                            // Keep seeking true; will be cleared after first frame
                        }
                        Err(e) => {
                            eprintln!("[INGEST] Seek failed: {}", e);
                            seeking = false; // clear on failure
                        }
                    }
                } else {
                    eprintln!("[INGEST] Seek already in progress, ignoring command");
                }
            }

            // Pull a frame
            match source.pull_frame() {
                Some((rgba, pts_ns)) => {
                    if let Some(packed) = pool.try_claim() {
                        pool.with_payload_mut(packed, |payload| {
                            payload.copy_from_slice(&rgba);
                        });
                        pool.set_pts_ns(packed, pts_ns);
                        let current_gen = seek_gen.load(Ordering::Acquire);
                        pool.set_seek_gen(packed, current_gen);

                        while let Err(_) = ingest_queue.push(packed) {
                            std::thread::sleep(std::time::Duration::from_micros(100));
                        }
                    } else {
                        std::thread::sleep(std::time::Duration::from_micros(100));
                    }

                    // If we were seeking, the first frame after seek is now enqueued
                    if seeking {
                        seeking = false;
                    }
                }
                None => {
                    // If we are seeking, do not break; clear the flag and continue.
                    if seeking {
                        eprintln!("[INGEST] No frame after seek, clearing seeking flag");
                        seeking = false;
                        // Continue the loop to allow new seek commands
                    } else {
                        // End of stream – break
                        break;
                    }
                    // Avoid busy loop when no frame is available
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
            }
        }
    }

    // Private ML loop (unchanged)
    fn ml_loop(
        pool: Arc<SlotPool<VIDEO_SLOT_SIZE>>,
        ingest_queue: Arc<ArrayQueue<PackedIndex>>,
        buffer: Arc<MediaBuffer>,
        model: Arc<PeopleSegFilter>,
        seek_gen: Arc<AtomicU64>,
        running: Arc<AtomicBool>,
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
                    pool.discard_slot(packed);
                }
            } else {
                std::thread::sleep(std::time::Duration::from_micros(100));
            }
        }
    }
}
