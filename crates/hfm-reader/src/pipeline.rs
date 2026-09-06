//! Pipeline orchestrator: ties source, buffer, and filter together with threading.

use crate::buffer::{TextBuffer, TextBufferImpl};
use crate::filter::TextFilter;
use crate::source::{TextSource, PullOutcome};
use crate::types::{Offset, RawChunk, ReaderError};
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use hfm_core::coordination::SeekGeneration;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Commands that can be sent to the pipeline.
#[derive(Debug, Clone)]
pub enum PipelineCommand {
    /// Seek to a specific offset (in the current processed document).
    Seek(Offset),
    /// Pause the pipeline (source and worker stop pulling/processing).
    Pause,
    /// Resume the pipeline.
    Resume,
    /// Stop the pipeline and shut down threads.
    Stop,
}

/// Current state of the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineState {
    Idle,
    Running,
    Paused,
    Stopped,
}

/// The pipeline controller.
///
/// Owns the source, buffer, filter, and manages worker threads.
/// Provides a synchronous API to send commands and read the buffer.
pub struct PipelineController {
    source: Mutex<Option<Box<dyn TextSource>>>,
    buffer: Arc<TextBufferImpl>,
    filter: Arc<dyn TextFilter>,
    generation: Arc<SeekGeneration>,
    running: Arc<AtomicBool>,
    state: Mutex<PipelineState>,
    cmd_tx: Sender<PipelineCommand>,
    cmd_rx: Receiver<PipelineCommand>,
    // Thread handles (joined on drop).
    pump_handle: Option<thread::JoinHandle<()>>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

impl PipelineController {
    /// Create a new pipeline controller.
    ///
    /// The pipeline is initially idle. Call `start()` to start the threads.
    pub fn new(
        source: Box<dyn TextSource>,
        buffer: Arc<TextBufferImpl>,
        filter: impl TextFilter + 'static,
    ) -> Self {
        let (cmd_tx, cmd_rx) = bounded(16);
        let running = Arc::new(AtomicBool::new(false));
        let generation = Arc::new(SeekGeneration::new());
        let filter = Arc::new(filter);

        Self {
            source: Mutex::new(Some(source)),
            buffer,
            filter,
            generation,
            running,
            state: Mutex::new(PipelineState::Idle),
            cmd_tx,
            cmd_rx,
            pump_handle: None,
            worker_handle: None,
        }
    }

    /// Start the pipeline threads.
    ///
    /// The source will be consumed (taken out of the Option).
    pub fn start(&mut self) -> Result<(), ReaderError> {
        let mut state = self.state.lock();
        if *state != PipelineState::Idle && *state != PipelineState::Stopped {
            return Err(ReaderError::Internal("Pipeline already running".to_string()));
        }

        // Take the source out.
        let source = self.source.lock().take().ok_or_else(|| {
            ReaderError::Internal("Source already taken; pipeline already started".to_string())
        })?;

        self.running.store(true, Ordering::Release);
        *state = PipelineState::Running;

        // Clone Arcs for threads.
        let buffer = self.buffer.clone();
        let filter = self.filter.clone();
        let generation = self.generation.clone();
        let running = self.running.clone();
        let cmd_rx = self.cmd_rx.clone();

        // Channels for raw chunks.
        let (raw_tx, raw_rx) = bounded::<RawChunk>(128);

        // Spawn pump thread.
        let pump_handle = {
            let mut source = source;
            let generation = generation.clone();
            let running = running.clone();
            let raw_tx = raw_tx.clone();

            thread::Builder::new()
                .name("reader-pump".to_string())
                .spawn(move || {
                    while running.load(Ordering::Acquire) {
                        // Check for pause state via a shared atomic? We'll use the generation
                        // and a separate pause flag. For simplicity, we'll use the command channel
                        // but we need to poll it. We'll use a non-blocking recv_timeout.
                        // Actually, the pump should check the state; we can use a shared atomic for pause.
                        // For now, we'll use a simple approach: if we receive a pause command, we pause.
                        // We'll handle commands in the main controller thread, not here.
                        // The pump just pulls raw chunks and sends them.
                        match source.try_pull_chunk(Duration::from_millis(10)) {
                            PullOutcome::Chunk(mut chunk) => {
                                // Stamp the chunk with the current generation.
                                let current_gen = generation.current();
                                chunk.generation = current_gen;
                                let _ = raw_tx.send(chunk); // Ignore send errors (worker may be gone).
                            }
                            PullOutcome::Empty => {
                                // Wait briefly and loop.
                                thread::sleep(Duration::from_millis(1));
                            }
                            PullOutcome::Eos => {
                                // End of file; we'll keep running but return empty.
                                // We could break, but then we'd need to restart on seek.
                                // We'll just keep polling and return Empty.
                                // We can also break and the worker will eventually stop.
                                // For now, we'll set a flag or just continue.
                                // To avoid busy-waiting, we sleep.
                                thread::sleep(Duration::from_millis(100));
                            }
                        }
                    }
                })
                .expect("Failed to spawn pump thread")
        };

        // Spawn worker thread.
        let worker_handle = {
            let buffer = buffer.clone();
            let filter = filter.clone();
            let generation = generation.clone();
            let running = running.clone();
            let raw_rx = raw_rx;

            thread::Builder::new()
                .name("reader-worker".to_string())
                .spawn(move || {
                    while running.load(Ordering::Acquire) {
                        match raw_rx.recv_timeout(Duration::from_millis(100)) {
                            Ok(raw) => {
                                // Check generation.
                                if raw.generation != generation.current() {
                                    continue;
                                }
                                // Process.
                                match filter.process(&raw) {
                                    Ok(processed) => {
                                        // Apply to buffer.
                                        if let Err(e) = buffer.apply_processed(processed) {
                                            eprintln!("Worker: apply_processed error: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Worker: filter error: {}", e);
                                    }
                                }
                            }
                            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                                // Continue loop.
                            }
                            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                                // Sender gone; exit.
                                break;
                            }
                        }
                    }
                })
                .expect("Failed to spawn worker thread")
        };

        self.pump_handle = Some(pump_handle);
        self.worker_handle = Some(worker_handle);

        Ok(())
    }

    /// Send a command to the pipeline.
    pub fn send_command(&self, cmd: PipelineCommand) -> Result<(), crossbeam_channel::SendError<PipelineCommand>> {
        self.cmd_tx.send(cmd)
    }

    /// Get a reference to the buffer.
    pub fn buffer(&self) -> Arc<TextBufferImpl> {
        self.buffer.clone()
    }

    /// Get the current pipeline state.
    pub fn state(&self) -> PipelineState {
        *self.state.lock()
    }

    /// Stop the pipeline and join threads.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        let _ = self.cmd_tx.send(PipelineCommand::Stop);
        if let Some(handle) = self.pump_handle.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        *self.state.lock() = PipelineState::Stopped;
    }
}

impl Drop for PipelineController {
    fn drop(&mut self) {
        self.stop();
    }
}