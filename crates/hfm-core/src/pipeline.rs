//! Pipeline orchestrating the ingest, ML processing, and buffering.
//! This module is source‑agnostic; it works with any implementor of `FrameSource`.

use crate::buffer::{MediaBuffer, Pts, VideoFrame};
use crate::coordination::SeekGeneration;
use crate::filter::VideoFilter;
use crate::memory::{PackedIndex, SlotPool};
use crate::ml::PeopleSegFilter;
use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

/// Constants for the video resolution (must match the model's expected input size).
pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 540;
pub const VIDEO_SLOT_SIZE: usize = (WIDTH * HEIGHT * 4) as usize;
pub const N_V: usize = 128;

/// A source of video frames.
pub trait FrameSource: Send + Sync {
    /// Try to pull a frame within `timeout`.
    ///
    /// Returns `Frame(data, pts)` when a frame is available, `Empty` when
    /// no frame arrived before the timeout, and `Eos` when the stream ended.
    fn try_pull_frame(&mut self, timeout: Duration) -> PullOutcome;
    fn seek(&mut self, delta_ns: i64) -> Result<(), String>;
}

/// Result from `FrameSource::try_pull_frame`.
pub enum PullOutcome {
    Frame(Vec<u8>, u64),
    Empty,
    Eos,
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
    Playing,
    Seeking { epoch: u64 },
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
    ml_tx: Option<Sender<PackedIndex>>,
    ml_rx: Option<Receiver<PackedIndex>>,
    cmd_rx: Receiver<PipelineCommand>,
    cmd_tx: Sender<PipelineCommand>,
    _ml_handle: Option<thread::JoinHandle<()>>,
    _controller_handle: Option<thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
    generation: Arc<SeekGeneration>,
    filter_enabled: bool,
}

impl PipelineController {
    pub fn new(
        source: Box<dyn FrameSource>,
        model: PeopleSegFilter,
        generation: Arc<SeekGeneration>,
        filter_enabled: bool,
    ) -> Self {
        let pool = Arc::new(SlotPool::<VIDEO_SLOT_SIZE>::new(N_V));
        let buffer = Arc::new(MediaBuffer::new(5.0, 30.0, 44100, 2048));
        let (ml_tx, ml_rx) = bounded::<PackedIndex>(N_V);
        let running = Arc::new(AtomicBool::new(true));
        let (cmd_tx, cmd_rx) = bounded(32);

        PipelineController {
            state: PipelineState::Idle,
            source: Some(source),
            model: Some(Arc::new(model)),
            buffer: buffer.clone(),
            slot_pool: Some(pool),
            ml_tx: Some(ml_tx),
            ml_rx: Some(ml_rx),
            generation,
            cmd_rx,
            cmd_tx,
            _ml_handle: None,
            _controller_handle: None,
            running,
            filter_enabled,
        }
    }

    pub fn send_command(
        &self,
        cmd: PipelineCommand,
    ) -> Result<(), crossbeam_channel::SendError<PipelineCommand>> {
        self.cmd_tx.send(cmd)
    }

    pub fn pop_processed_frame(&self) -> Option<VideoFrame> {
        let frame = self.buffer.pop_video()?;

        if let Some(pool) = self.slot_pool.as_ref() {
            pool.discard_slot(frame.slot);
        }

        Some(frame)
    }

    /// Return the PTS of the next processed video frame without removing it.
    pub fn peek_video_pts(&self) -> Option<u64> {
        self.buffer.peek_video_pts()
    }

    pub fn start(&mut self) {
        let source = self.source.take().expect("Controller already started");
        let model = self.model.take().expect("Controller already started");
        let buffer = self.buffer.clone();
        let slot_pool = self.slot_pool.clone().expect("Controller already started");
        let ml_tx = self.ml_tx.take().expect("Controller already started");
        let ml_rx = self.ml_rx.take().expect("Controller already started");
        let cmd_rx = self.cmd_rx.clone();
        let running = self.running.clone();

        self.state = PipelineState::Stopped;

        // Spawn ML thread
        let ml_slot_pool = slot_pool.clone();
        let ml_model = model;
        let ml_buffer = buffer.clone();
        let ml_running = running.clone();
        let filter_enabled = self.filter_enabled;
        let ml_handle = std::thread::Builder::new()
            .name("ml-worker".to_string())
            .spawn(move || {
                Self::ml_thread(
                    ml_rx,
                    ml_slot_pool,
                    ml_model,
                    ml_buffer,
                    filter_enabled,
                    ml_running,
                );
            })
            .expect("Failed to spawn ML thread");

        let controller_slot_pool = slot_pool.clone();
        let controller_ml_tx = ml_tx.clone();
        let controller_generation = self.generation.clone();

        let controller_handle = std::thread::Builder::new()
            .name("controller".to_string())
            .spawn(move || {
                Self::run_loop(
                    source,
                    buffer,
                    controller_generation,
                    controller_slot_pool,
                    controller_ml_tx,
                    cmd_rx,
                    running,
                );
            })
            .expect("Failed to spawn controller thread");

        self._ml_handle = Some(ml_handle);
        self._controller_handle = Some(controller_handle);
    }

    // ML thread: consumes from blocking queue, runs inference, pushes to buffer.
    fn ml_thread(
        ml_rx: Receiver<PackedIndex>,
        slot_pool: Arc<SlotPool<VIDEO_SLOT_SIZE>>,
        model: Arc<PeopleSegFilter>,
        buffer: Arc<MediaBuffer>,
        filter_enabled: bool,
        running: Arc<AtomicBool>,
    ) {
        while running.load(Ordering::Acquire) {
            match ml_rx.recv() {
                Ok(packed) => {
                    let _ = slot_pool.with_payload_mut(packed, |payload| {
                        if filter_enabled {
                            if let Err(e) = model.filter_frame(payload, WIDTH, HEIGHT) {
                                eprintln!("[ML] filter_frame failed: {e}");
                            }
                        }
                    });
                    let pts_ns = slot_pool.get_pts_ns(packed);
                    let pts = Pts(pts_ns);
                    let data = slot_pool.with_payload_mut(packed, |p| p.to_vec());
                    let seek_gen = slot_pool.get_seek_gen(packed);
                    let frame = VideoFrame {
                        pts,
                        slot: packed,
                        data,
                        seek_gen,
                    };

                    let mut frame = frame;
                    loop {
                        // Break immediately if pipeline is shutting down
                        if !running.load(Ordering::Acquire) {
                            slot_pool.discard_slot(frame.slot);
                            break;
                        }

                        // If the frame is from an old generation, discard it.
                        if frame.seek_gen != buffer.current_seek_epoch() {
                            slot_pool.discard_slot(frame.slot);
                            break;
                        }

                        match buffer.push_video(frame) {
                            Ok(()) => break,
                            Err(returned) => {
                                if returned.seek_gen != buffer.current_seek_epoch() {
                                    slot_pool.discard_slot(returned.slot);
                                    break;
                                }
                                frame = returned;
                                thread::sleep(Duration::from_millis(1));
                            }
                        }
                    }
                }
                Err(_) => break,
            }
        }
    }

    // Helper: enqueue a frame (write to slot, set generation, send to ML queue).
    fn enqueue_frame(
        rgba: Vec<u8>,
        pts_ns: u64,
        seek_gen: u64,
        slot_pool: &Arc<SlotPool<VIDEO_SLOT_SIZE>>,
        ml_tx: &Sender<PackedIndex>,
        running: &Arc<AtomicBool>,
    ) {
        let packed = loop {
            if !running.load(Ordering::Acquire) {
                return;
            }
            if let Some(packed) = slot_pool.try_claim() {
                break packed;
            }
            thread::sleep(Duration::from_millis(1));
        };

        slot_pool.with_payload_mut(packed, |payload| {
            payload.copy_from_slice(&rgba);
        });
        slot_pool.set_pts_ns(packed, pts_ns);
        slot_pool.set_seek_gen(packed, seek_gen);

        if ml_tx.send(packed).is_err() {
            slot_pool.discard_slot(packed);
        }
    }

    // Controller loop – owns source, state, and pulls frames.
    fn run_loop(
        mut source: Box<dyn FrameSource>,
        buffer: Arc<MediaBuffer>,
        generation: Arc<SeekGeneration>,
        slot_pool: Arc<SlotPool<VIDEO_SLOT_SIZE>>,
        ml_tx: Sender<PackedIndex>,
        cmd_rx: Receiver<PipelineCommand>,
        running: Arc<AtomicBool>,
    ) {
        println!("[CONTROLLER] Run loop started");

        while running.load(Ordering::Acquire) {
            match cmd_rx.try_recv() {
                Ok(cmd) => match cmd {
                    PipelineCommand::Seek(delta) => {
                        // Increment the seek generation inside the controller
                        let new_gen = generation.increment();
                        let discarded = buffer.flush_to(new_gen);

                        // Release slots for frames that were sitting in the buffer.
                        for frame in discarded {
                            slot_pool.discard_slot(frame.slot);
                        }

                        if let Err(e) = source.seek(delta.to_i64()) {
                            eprintln!("[CONTROLLER] Seek failed: {}", e);
                        }
                    }
                    PipelineCommand::Stop => break,
                },
                Err(TryRecvError::Empty) => {
                    match source.try_pull_frame(Duration::from_millis(15)) {
                        PullOutcome::Frame(rgba, pts_ns) => {
                            let epoch = buffer.current_seek_epoch();
                            Self::enqueue_frame(rgba, pts_ns, epoch, &slot_pool, &ml_tx, &running);
                        }
                        PullOutcome::Eos => break,
                        PullOutcome::Empty => {
                            // No frame yet. Loop again and check for commands.
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => break,
            }
        }

        println!("[CONTROLLER] Run loop exiting");
    }

    /// Shut down the pipeline, signal threads to exit, and join in producer-first order.
    pub fn shutdown(&mut self) {
        // 1. Signal running flag to false
        self.running.store(false, Ordering::Release);

        // 2. Wake controller thread if waiting on commands
        let _ = self.cmd_tx.send(PipelineCommand::Stop);

        // 3. Join upstream controller thread FIRST.
        // Terminating this thread drops its internal `ml_tx` sender.
        if let Some(handle) = self._controller_handle.take() {
            let _ = handle.join();
        }

        // 4. Join downstream ML thread SECOND.
        // `ml_rx.recv()` receives Err immediately because all senders are disconnected.
        if let Some(handle) = self._ml_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for PipelineController {
    fn drop(&mut self) {
        self.shutdown();
    }
}
