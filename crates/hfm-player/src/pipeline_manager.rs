//! Manages the media pipeline: GStreamer source, pumps, and controller.

use crate::audio_output::spawn_audio_output;
use crate::config::*;
use crate::gst_source::GstSource;
use crossbeam_channel::{Receiver, Sender, bounded};
use hfm_core::coordination::{AudioClock, BufferingFlag, SeekGeneration};
use hfm_core::media_messages::{ProcessedAudioChunk, RawAudioChunk, RawVideoFrame};
use hfm_core::ml::{DemucsConfig, PeopleSegFilter, spawn_demucs_worker};
use hfm_core::ml::{ExecutionProvider, SessionConfig};
use hfm_core::pipeline::{
    FrameSource, PipelineCommand, PipelineController, PullOutcome, SeekDelta,
};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Adapter that turns `Receiver<RawVideoFrame>` into `hfm_core::FrameSource`.
pub struct ChannelVideoSource {
    rx: Receiver<RawVideoFrame>,
    generation: Arc<SeekGeneration>,
}

impl FrameSource for ChannelVideoSource {
    fn try_pull_frame(&mut self, timeout: Duration) -> PullOutcome {
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return PullOutcome::Empty;
            }
            let remaining = deadline - now;
            match self.rx.recv_timeout(remaining) {
                Ok(frame) => {
                    if frame.generation == self.generation.current() {
                        return PullOutcome::Frame(frame.data, frame.pts_ns);
                    }
                    // Stale – discard and keep waiting.
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    return PullOutcome::Empty;
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    return PullOutcome::Eos;
                }
            }
        }
    }

    fn seek(&mut self, _delta_ns: i64) -> Result<(), String> {
        Ok(())
    }
}

pub fn spawn_video_pump(
    gst_source: Arc<Mutex<GstSource>>,
    video_tx: Sender<RawVideoFrame>,
    generation: Arc<SeekGeneration>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("video-pump".to_string())
        .spawn(move || {
            while running.load(Ordering::Acquire) {
                let (maybe_frame, current_gen) = {
                    let source = gst_source.lock();
                    let frame = source.try_pull_video_frame(Duration::from_millis(5));
                    let current_generation = generation.current();
                    (frame, current_generation)
                };
                match maybe_frame {
                    Some((data, pts_ns)) => {
                        let msg = RawVideoFrame {
                            data,
                            pts_ns,
                            generation: current_gen,
                        };
                        if video_tx.send(msg).is_err() {
                            break;
                        }
                    }
                    None => {
                        let source = gst_source.lock();
                        let eos = source.is_video_eos();
                        drop(source);
                        if eos {
                            println!("[PUMP] video EOS");
                            break;
                        }
                        thread::sleep(Duration::from_millis(1));
                    }
                }
            }
        })
        .expect("Failed to spawn video pump")
}

pub fn spawn_audio_pump(
    gst_source: Arc<Mutex<GstSource>>,
    audio_tx: Sender<RawAudioChunk>,
    generation: Arc<SeekGeneration>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("audio-pump".to_string())
        .spawn(move || {
            while running.load(Ordering::Acquire) {
                let (maybe_chunk, current_gen) = {
                    let source = gst_source.lock();
                    let chunk = source.try_pull_audio_frame(Duration::from_millis(5));
                    let current_generation = generation.current();
                    (chunk, current_generation)
                };
                match maybe_chunk {
                    Some((samples, pts_ns)) => {
                        let msg = RawAudioChunk {
                            samples,
                            pts_ns,
                            generation: current_gen,
                        };
                        if audio_tx.send(msg).is_err() {
                            break;
                        }
                    }
                    None => {
                        let source = gst_source.lock();
                        let eos = source.is_audio_eos();
                        drop(source);
                        if eos {
                            println!("[PUMP] audio EOS");
                            break;
                        }
                        thread::sleep(Duration::from_millis(1));
                    }
                }
            }
        })
        .expect("Failed to spawn audio pump")
}

/// Audio passthrough: converts raw audio chunks to processed chunks without modification.
pub fn spawn_audio_passthrough(
    raw_rx: Receiver<RawAudioChunk>,
    processed_tx: Sender<ProcessedAudioChunk>,
    generation: Arc<SeekGeneration>,
) -> thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("audio-passthrough".to_string())
        .spawn(move || {
            while let Ok(chunk) = raw_rx.recv() {
                if chunk.generation != generation.current() {
                    continue; // stale
                }
                let processed = ProcessedAudioChunk {
                    samples: chunk.samples,
                    pts_ns: chunk.pts_ns,
                    generation: chunk.generation,
                };
                if processed_tx.send(processed).is_err() {
                    break;
                }
            }
        })
        .expect("Failed to spawn audio passthrough")
}

/// Manages the entire media pipeline.
pub struct PipelineManager {
    gst_source: Option<Arc<Mutex<GstSource>>>,
    pipeline: Option<PipelineController>,
    video_pump: Option<thread::JoinHandle<()>>,
    audio_pump: Option<thread::JoinHandle<()>>,
    audio_processor: Option<thread::JoinHandle<()>>,
    audio_output: Option<thread::JoinHandle<()>>,
    pub has_audio: bool,

    generation: Arc<SeekGeneration>,
    audio_clock: Arc<AudioClock>,
    buffering: Arc<BufferingFlag>,
    audio_clear_requested: Arc<AtomicBool>,
    volume_atomic: Arc<AtomicU8>,
    pump_running: Arc<AtomicBool>,
    is_playing: Arc<AtomicBool>,
}

impl PipelineManager {
    pub fn new(
        generation: Arc<SeekGeneration>,
        audio_clock: Arc<AudioClock>,
        buffering: Arc<BufferingFlag>,
        audio_clear_requested: Arc<AtomicBool>,
        volume_atomic: Arc<AtomicU8>,
        is_playing: Arc<AtomicBool>,
    ) -> Self {
        Self {
            gst_source: None,
            pipeline: None,
            video_pump: None,
            audio_pump: None,
            audio_processor: None,
            audio_output: None,
            has_audio: false,
            generation,
            audio_clock,
            buffering,
            audio_clear_requested,
            volume_atomic,
            pump_running: Arc::new(AtomicBool::new(true)),
            is_playing,
        }
    }

    /// Build a SessionConfig for the video model (PPHumanSeg) based on the selected backend.
    fn build_video_config(&self, backend: crate::gui::Backend) -> SessionConfig {
        let mut config = SessionConfig::video_default();
        config.provider = match backend {
            crate::gui::Backend::Cpu => ExecutionProvider::Cpu,
            crate::gui::Backend::DirectML => {
                #[cfg(target_os = "windows")]
                {
                    ExecutionProvider::DirectML
                }
                #[cfg(not(target_os = "windows"))]
                {
                    eprintln!("DirectML is not supported on this platform. Falling back to CPU.");
                    ExecutionProvider::Cpu
                }
            }
            crate::gui::Backend::OpenVINO => {
                #[cfg(any(target_os = "linux", target_os = "windows"))]
                {
                    ExecutionProvider::OpenVINO {
                        device: "GPU".to_string(),
                    }
                }
                #[cfg(not(any(target_os = "linux", target_os = "windows")))]
                {
                    eprintln!(
                        "OpenVINO is only supported on Linux and Windows. Falling back to CPU."
                    );
                    ExecutionProvider::Cpu
                }
            }
            crate::gui::Backend::CoreML => {
                #[cfg(target_vendor = "apple")]
                {
                    ExecutionProvider::CoreML
                }
                #[cfg(not(target_vendor = "apple"))]
                {
                    eprintln!("CoreML is only supported on Apple platforms. Falling back to CPU.");
                    ExecutionProvider::Cpu
                }
            }
        };
        config
    }

    /// Build a DemucsConfig for the audio model (HT-Demucs) based on the selected backend.
    fn build_audio_config(&self, backend: crate::gui::Backend) -> DemucsConfig {
        let model_path = format!(
            "{}/../hfm-core/models/htdemucs_ft_vocals_fp16weights.onnx",
            env!("CARGO_MANIFEST_DIR")
        );
        let provider = match backend {
            crate::gui::Backend::Cpu => ExecutionProvider::Cpu,
            crate::gui::Backend::DirectML => {
                #[cfg(target_os = "windows")]
                {
                    ExecutionProvider::DirectML
                }
                #[cfg(not(target_os = "windows"))]
                {
                    eprintln!("DirectML not supported; falling back to CPU.");
                    ExecutionProvider::Cpu
                }
            }
            crate::gui::Backend::OpenVINO => {
                #[cfg(any(target_os = "windows", target_os = "linux"))]
                {
                    ExecutionProvider::OpenVINO {
                        device: "GPU".to_string(),
                    }
                }
                #[cfg(not(any(target_os = "windows", target_os = "linux")))]
                {
                    eprintln!("OpenVINO not supported; falling back to CPU.");
                    ExecutionProvider::Cpu
                }
            }
            crate::gui::Backend::CoreML => {
                #[cfg(target_vendor = "apple")]
                {
                    ExecutionProvider::CoreML
                }
                #[cfg(not(target_vendor = "apple"))]
                {
                    eprintln!("CoreML not supported; falling back to CPU.");
                    ExecutionProvider::Cpu
                }
            }
        };
        DemucsConfig {
            model_path,
            provider,
            window_size: WINDOW_SAMPLES,
        }
    }

    /// Stops all running pipeline threads, joins handles, and resets audio clocks.
    pub fn stop(&mut self) {
        // 1. Signal pump threads to stop spinning on try_pull / empty sleep loops
        self.pump_running.store(false, Ordering::Release);

        // 2. Shutdown the pipeline controller first
        if let Some(mut controller) = self.pipeline.take() {
            controller.shutdown();
        }

        // 3. Drop GStreamer source handle
        self.gst_source = None;

        // 4. Safely join all pump, processing, and output worker threads
        if let Some(handle) = self.video_pump.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.audio_pump.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.audio_processor.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.audio_output.take() {
            let _ = handle.join();
        }

        self.has_audio = false;
        self.buffering.set(true);
        self.audio_clock.reset();
        self.audio_clear_requested.store(true, Ordering::SeqCst);
    }

    /// Restart the pipeline with the given parameters.
    /// Returns the media duration in nanoseconds.
    pub fn restart(
        &mut self,
        video_path: PathBuf,
        video_backend: crate::gui::Backend,
        audio_backend: crate::gui::Backend,
        filter_enabled: bool,
        audio_enabled: bool,
    ) -> Result<u64, String> {
        self.stop();

        // Re-enable pump running flag for the new pipeline run
        self.pump_running.store(true, Ordering::Release);

        // Build GStreamer source
        let gst_source = Arc::new(Mutex::new(
            GstSource::new(&video_path.to_string_lossy())
                .map_err(|e| format!("Failed to create GStreamer source: {}", e))?,
        ));
        self.gst_source = Some(gst_source.clone());

        // Video pump
        let (video_tx, video_rx) = bounded(4);
        let video_pump = spawn_video_pump(
            gst_source.clone(),
            video_tx,
            self.generation.clone(),
            self.pump_running.clone(),
        );
        self.video_pump = Some(video_pump);

        let video_source = ChannelVideoSource {
            rx: video_rx,
            generation: self.generation.clone(),
        };

        // Load video model (hardcoded path)
        let model_path = format!(
            "{}/../hfm-core/models/pphumanseg.onnx",
            env!("CARGO_MANIFEST_DIR")
        );
        let video_config = self.build_video_config(video_backend);
        let model = PeopleSegFilter::new(&model_path, Some(video_config))
            .map_err(|e| format!("Failed to load video model: {}", e))?;

        let mut pipeline = PipelineController::new(
            Box::new(video_source),
            model,
            self.generation.clone(),
            filter_enabled,
        );
        pipeline.start();
        self.pipeline = Some(pipeline);

        // Pipeline starts in Paused state, so set is_playing to false.
        // The user must click Play to resume.
        self.is_playing.store(false, Ordering::Release);

        // Audio handling
        if audio_enabled {
            let audio_config = self.build_audio_config(audio_backend);

            let (raw_audio_tx, raw_audio_rx) = bounded(128);
            let audio_pump = spawn_audio_pump(
                gst_source.clone(),
                raw_audio_tx,
                self.generation.clone(),
                self.pump_running.clone(),
            );
            self.audio_pump = Some(audio_pump);

            let (processed_audio_tx, processed_audio_rx) = bounded(128);
            let audio_processor = spawn_demucs_worker(
                audio_config,
                raw_audio_rx,
                processed_audio_tx,
                self.generation.clone(),
            );
            self.audio_processor = Some(audio_processor);

            let audio_output = spawn_audio_output(
                processed_audio_rx,
                self.audio_clock.clone(),
                self.buffering.clone(),
                self.generation.clone(),
                self.audio_clear_requested.clone(),
                SAMPLE_RATE,
                CHANNELS,
                self.volume_atomic.clone(),
                self.is_playing.clone(),
            );
            self.audio_output = Some(audio_output);
            self.has_audio = true;

            if !audio_enabled || !self.has_audio {
                self.buffering.set(false);
            }
        } else {
            // Audio disabled: consume audio sink to prevent GStreamer stalling, but discard data.
            let (raw_audio_tx, raw_audio_rx) = bounded(128);
            let audio_pump = spawn_audio_pump(
                gst_source.clone(),
                raw_audio_tx,
                self.generation.clone(),
                self.pump_running.clone(),
            );
            self.audio_pump = Some(audio_pump);

            // Spawn a thread to receive and discard audio chunks.
            let discard_handle = std::thread::spawn(move || {
                while let Ok(_chunk) = raw_audio_rx.recv() {
                    // Discard
                }
            });
            self.audio_processor = Some(discard_handle);

            self.has_audio = false;
            self.buffering.set(false);
        }

        // Query duration
        let duration = gst_source.lock().duration_ns().unwrap_or(0);
        Ok(duration)
    }

    /// Pause playback.
    pub fn pause_playback(&mut self) -> Result<(), String> {
        self.is_playing.store(false, Ordering::Release);
        if let Some(source) = &self.gst_source {
            let source = source.lock();
            source.pause().map_err(|e| format!("Pause failed: {}", e))
        } else {
            Err("No GStreamer source".to_string())
        }
    }

    /// Resume playback.
    pub fn resume_playback(&mut self) -> Result<(), String> {
        self.is_playing.store(true, Ordering::Release);
        if let Some(source) = &self.gst_source {
            let source = source.lock();
            source.resume().map_err(|e| format!("Resume failed: {}", e))
        } else {
            Err("No GStreamer source".to_string())
        }
    }

    /// Send a seek command to the pipeline.
    pub fn seek(&mut self, delta: SeekDelta) -> Result<(), String> {
        if let Some(source) = self.gst_source.as_ref() {
            let mut source = source.lock();
            source
                .seek(delta.to_i64())
                .map_err(|e| format!("GStreamer seek failed: {}", e))?;
        }
        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline
                .send_command(PipelineCommand::Seek(delta))
                .map_err(|e| format!("Pipeline seek failed: {:?}", e))?;
        }
        Ok(())
    }

    /// Get the next video frame PTS (without popping).
    pub fn peek_video_pts(&self) -> Option<u64> {
        self.pipeline.as_ref().and_then(|p| p.peek_video_pts())
    }

    /// Pop the next processed video frame.
    pub fn pop_processed_frame(&mut self) -> Option<hfm_core::VideoFrame> {
        self.pipeline.as_mut().and_then(|p| p.pop_processed_frame())
    }

    /// Check if buffering.
    pub fn is_buffering(&self) -> bool {
        self.buffering.is_buffering()
    }
}
