//! Pipeline orchestrating the ingest, ML processing, and buffering.
//! This module is source‑agnostic; it works with any implementor of `FrameSource`.

use crate::buffer::{MediaBuffer, Pts, VideoFrame};
use crate::filter::VideoFilter;
use crate::memory::{PackedIndex, SlotPool};
use crate::ml::PeopleSegFilter;
use crossbeam::queue::ArrayQueue;
use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// Direction and amount for a seek.
#[derive(Debug, Clone, Copy)]
pub enum SeekDelta {
    Forward(u64),
    Backward(u64),
}

impl SeekDelta {
    pub fn to_i64(&self) -> i64 {
        match self {
            SeekDelta::Forward(delta) => *delta as i64,
            SeekDelta::Backward(delta) => -(*delta as i64),
        }
    }
}

/// Possible states of the pipeline.
#[derive(Debug)]
pub enum PipelineState {
    Idle,
    Playing {
        generation: u64,
    },
    Seeking {
        generation: u64,
        pending_delta: i64,
        initiated: bool,
    },
    Stopped,
}

/// Commands sent to the pipeline controller.
#[derive(Debug)]
pub enum PipelineCommand {
    Seek(SeekDelta),
    Stop,
}

/// The pipeline controller. Owns the state, source, buffer, and ML queue.
pub struct PipelineController {
    state: PipelineState,
    source: Option<Box<dyn FrameSource>>,
    model: Option<Arc<PeopleSegFilter>>,
    buffer: Arc<MediaBuffer>,
    slot_pool: Option<Arc<SlotPool<VIDEO_SLOT_SIZE>>>,
    ml_queue: Option<Arc<ArrayQueue<PackedIndex>>>,
    cmd_rx: Receiver<PipelineCommand>,
    cmd_tx: Sender<PipelineCommand>,
    _ml_handle: Option<std::thread::JoinHandle<()>>,
    _controller_handle: Option<std::thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
}

impl PipelineController {
    pub fn new(source: Box<dyn FrameSource>, model: PeopleSegFilter) -> Self {
        let pool = Arc::new(SlotPool::<VIDEO_SLOT_SIZE>::new(N_V));
        let buffer = Arc::new(MediaBuffer::new(5.0, 30.0, 44100, 2048));
        let ml_queue = Arc::new(ArrayQueue::<PackedIndex>::new(N_V));
        let running = Arc::new(AtomicBool::new(true));
        let (tx, rx) = bounded(32);

        PipelineController {
            state: PipelineState::Idle,
            source: Some(source),
            model: Some(Arc::new(model)),
            buffer: buffer.clone(),
            slot_pool: Some(pool),
            ml_queue: Some(ml_queue),
            cmd_rx: rx,
            cmd_tx: tx,
            _ml_handle: None,
            _controller_handle: None,
            running,
        }
    }

    pub fn send_command(
        &self,
        cmd: PipelineCommand,
    ) -> Result<(), crossbeam_channel::SendError<PipelineCommand>> {
        self.cmd_tx.send(cmd)
    }

    pub fn pop_processed_frame(&self) -> Option<VideoFrame> {
        self.buffer.pop_video()
    }

    pub fn start(&mut self) {
        let source = self.source.take().expect("Controller already started");
        let model = self.model.take().expect("Controller already started");
        let buffer = self.buffer.clone();
        let slot_pool = self.slot_pool.take().expect("Controller already started");
        let ml_queue = self.ml_queue.take().expect("Controller already started");
        let cmd_rx = self.cmd_rx.clone();
        let running = self.running.clone();

        let state = std::mem::replace(&mut self.state, PipelineState::Stopped);

        // Spawn ML thread
        let ml_slot_pool = slot_pool.clone();
        let ml_model = model;
        let ml_queue_clone = ml_queue.clone();
        let ml_buffer = buffer.clone();
        let ml_running = running.clone();
        let ml_handle = std::thread::spawn(move || {
            Self::ml_thread(
                ml_queue_clone,
                ml_slot_pool,
                ml_model,
                ml_buffer,
                ml_running,
            );
        });

        // Spawn controller loop – owns source
        let controller_handle = std::thread::spawn(move || {
            Self::run_loop(state, source, buffer, slot_pool, ml_queue, cmd_rx, running);
        });

        self._ml_handle = Some(ml_handle);
        self._controller_handle = Some(controller_handle);
    }

    // ML thread: consumes from queue, runs inference, pushes to buffer.
    fn ml_thread(
        ml_queue: Arc<ArrayQueue<PackedIndex>>,
        slot_pool: Arc<SlotPool<VIDEO_SLOT_SIZE>>,
        model: Arc<PeopleSegFilter>,
        buffer: Arc<MediaBuffer>,
        running: Arc<AtomicBool>,
    ) {
        while running.load(Ordering::Acquire) {
            if let Some(packed) = ml_queue.pop() {
                let _ = slot_pool
                    .with_payload_mut(packed, |payload| model.filter_frame(payload, WIDTH, HEIGHT));
                let pts_ns = slot_pool.get_pts_ns(packed);
                let pts = Pts(pts_ns);
                let data = slot_pool.with_payload_mut(packed, |p| p.to_vec());
                let frame = VideoFrame {
                    pts,
                    slot: packed,
                    data,
                    seek_gen: 0, // buffer uses its own generation check
                };
                if let Err(_) = buffer.push_video(frame) {
                    slot_pool.discard_slot(packed);
                }
            } else {
                std::thread::sleep(std::time::Duration::from_micros(100));
            }
        }
    }

    // Helper: enqueue a frame (write to slot, set generation, push to ML queue)
    fn enqueue_frame(
        rgba: Vec<u8>,
        pts_ns: u64,
        generation: u64,
        slot_pool: &Arc<SlotPool<VIDEO_SLOT_SIZE>>,
        ml_queue: &Arc<ArrayQueue<PackedIndex>>,
    ) {
        if let Some(packed) = slot_pool.try_claim() {
            slot_pool.with_payload_mut(packed, |payload| {
                payload.copy_from_slice(&rgba);
            });
            slot_pool.set_pts_ns(packed, pts_ns);
            slot_pool.set_seek_gen(packed, generation);
            if let Err(_) = ml_queue.push(packed) {
                eprintln!("[CONTROLLER] ML queue full, dropping frame");
            }
        } else {
            eprintln!("[CONTROLLER] No free slot available");
        }
    }

    // Controller loop – owns source, state, and pulls frames.
    fn run_loop(
        mut state: PipelineState,
        mut source: Box<dyn FrameSource>,
        buffer: Arc<MediaBuffer>,
        slot_pool: Arc<SlotPool<VIDEO_SLOT_SIZE>>,
        ml_queue: Arc<ArrayQueue<PackedIndex>>,
        cmd_rx: Receiver<PipelineCommand>,
        running: Arc<AtomicBool>,
    ) {
        println!("[CONTROLLER] Run loop started");

        while running.load(Ordering::Acquire) {
            // Check for commands without blocking
            match cmd_rx.try_recv() {
                Ok(cmd) => match cmd {
                    PipelineCommand::Seek(delta) => match &mut state {
                        PipelineState::Idle => {
                            let delta_i64 = delta.to_i64();
                            if let Err(e) = source.seek(delta_i64) {
                                eprintln!("[CONTROLLER] Seek from idle failed: {}", e);
                            } else {
                                state = PipelineState::Seeking {
                                    generation: 1,
                                    pending_delta: delta_i64,
                                    initiated: true,
                                };
                            }
                        }
                        PipelineState::Playing { generation } => {
                            let new_generation = *generation + 1;
                            buffer.flush();
                            let delta_i64 = delta.to_i64();
                            if let Err(e) = source.seek(delta_i64) {
                                eprintln!("[CONTROLLER] Seek failed: {}", e);
                            } else {
                                state = PipelineState::Seeking {
                                    generation: new_generation,
                                    pending_delta: delta_i64,
                                    initiated: true,
                                };
                            }
                        }
                        PipelineState::Seeking { .. } => {
                            eprintln!("[CONTROLLER] Overlapping seek ignored");
                        }
                        PipelineState::Stopped => {
                            eprintln!("[CONTROLLER] Seek ignored (stopped)");
                        }
                    },
                    PipelineCommand::Stop => {
                        state = PipelineState::Stopped;
                        break;
                    }
                },
                Err(TryRecvError::Empty) => {
                    // No command; pull a frame if state allows
                    match &mut state {
                        PipelineState::Idle => {
                            if let Some((rgba, pts_ns)) = source.pull_frame() {
                                Self::enqueue_frame(rgba, pts_ns, 1, &slot_pool, &ml_queue);
                                state = PipelineState::Playing { generation: 1 };
                            } else {
                                // End of stream
                                state = PipelineState::Stopped;
                                break;
                            }
                        }
                        PipelineState::Playing { generation } => {
                            if let Some((rgba, pts_ns)) = source.pull_frame() {
                                Self::enqueue_frame(
                                    rgba,
                                    pts_ns,
                                    *generation,
                                    &slot_pool,
                                    &ml_queue,
                                );
                            } else {
                                state = PipelineState::Stopped;
                                break;
                            }
                        }
                        PipelineState::Seeking {
                            generation,
                            initiated,
                            ..
                        } => {
                            if *initiated {
                                // Accept first frame after seek
                                if let Some((rgba, pts_ns)) = source.pull_frame() {
                                    Self::enqueue_frame(
                                        rgba,
                                        pts_ns,
                                        *generation,
                                        &slot_pool,
                                        &ml_queue,
                                    );
                                    state = PipelineState::Playing {
                                        generation: *generation,
                                    };
                                } else {
                                    // No frame after seek (maybe end of stream)
                                    state = PipelineState::Stopped;
                                    break;
                                }
                            } else {
                                // Seek not yet initiated; should not happen as we set initiated in seek command.
                                // Just sleep to avoid busy loop.
                                std::thread::sleep(std::time::Duration::from_micros(100));
                            }
                        }
                        PipelineState::Stopped => {
                            break;
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => break,
            }
        }
        println!("[CONTROLLER] Run loop exiting");
    }
}
