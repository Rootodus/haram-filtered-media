mod audio_output;
mod gst_source;
mod gui;
mod renderer;

use crate::audio_output::spawn_audio_output;
use crate::gst_source::GstSource;
use crate::gui::{AppMode, AppState, Backend, Bridge, GuiCommand, PlaybackState};
use crate::renderer::Renderer;
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use hfm_core::coordination::{AudioClock, BufferingFlag, SeekGeneration};
use hfm_core::media_messages::{RawAudioChunk, RawVideoFrame};
use hfm_core::ml::{DemucsConfig, PeopleSegFilter, spawn_demucs_worker};
use hfm_core::pipeline::{
    FrameSource, PipelineCommand, PipelineController, PullOutcome, SeekDelta,
};
use mimalloc::MiMalloc;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 2;
const SEEK_DELTA_NS: i64 = 10_000_000_000;
const WINDOW_SAMPLES: usize = 343_980;
const SEEK_DEBOUNCE_MS: u64 = 200; // 200 ms between seeks

/// Adapter that turns `Receiver<RawVideoFrame>` into `hfm_core::FrameSource`.
struct ChannelVideoSource {
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
                    // Stale generation frame. Discard and keep waiting.
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

fn spawn_video_pump(
    gst_source: Arc<Mutex<GstSource>>,
    video_tx: Sender<RawVideoFrame>,
    generation: Arc<SeekGeneration>,
) -> thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("video-pump".to_string())
        .spawn(move || {
            loop {
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

fn spawn_audio_pump(
    gst_source: Arc<Mutex<GstSource>>,
    audio_tx: Sender<RawAudioChunk>,
    generation: Arc<SeekGeneration>,
) -> thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("audio-pump".to_string())
        .spawn(move || {
            loop {
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

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pipeline: Option<PipelineController>,

    gst_source: Option<Arc<Mutex<GstSource>>>,

    video_pump: Option<thread::JoinHandle<()>>,
    audio_pump: Option<thread::JoinHandle<()>>,
    audio_processor: Option<thread::JoinHandle<()>>,
    audio_output: Option<thread::JoinHandle<()>>,

    generation: Arc<SeekGeneration>,
    audio_clock: Arc<AudioClock>,
    buffering: Arc<BufferingFlag>,
    audio_clear_requested: Arc<AtomicBool>,

    has_audio: bool,
    frame_count: u32,
    fps_timer: Instant,
    last_frame: Option<Vec<u8>>,
    last_seek_time: Instant,

    state: Arc<Mutex<AppState>>,
    bridge: Bridge,
    cmd_rx: Receiver<GuiCommand>,
    volume_atomic: Arc<AtomicU8>,
}

impl App {
    fn new() -> Self {
        let (cmd_tx, cmd_rx) = unbounded();
        let bridge = Bridge::new(cmd_tx);
        let state = Arc::new(Mutex::new(AppState::default()));
        let volume_atomic = Arc::new(AtomicU8::new(state.lock().volume.get()));

        Self {
            window: None,
            renderer: None,
            pipeline: None,
            gst_source: None,
            video_pump: None,
            audio_pump: None,
            audio_processor: None,
            audio_output: None,
            generation: Arc::new(SeekGeneration::new()),
            audio_clock: Arc::new(AudioClock::new(SAMPLE_RATE)),
            buffering: Arc::new(BufferingFlag::new(true)),
            audio_clear_requested: Arc::new(AtomicBool::new(false)),
            has_audio: false,
            frame_count: 0,
            fps_timer: Instant::now(),
            last_frame: None,
            last_seek_time: Instant::now(),
            state,
            bridge,
            cmd_rx,
            volume_atomic,
        }
    }

    fn handle_gui_command(&mut self, cmd: GuiCommand) {
        match cmd {
            GuiCommand::LoadVideo(path) => {
                // Update state
                {
                    let mut state = self.state.lock();
                    state.video_path = Some(path);
                    // Reset playback state – pipeline will restart
                }
                self.restart_pipeline();
            }
            GuiCommand::LoadAudioModel(path) => {
                {
                    let mut state = self.state.lock();
                    state.audio_model_path = Some(path);
                }
                // We don't restart immediately, user must click Restart or load video.
                // But we can also restart automatically if desired? Let's keep it as is.
            }
            GuiCommand::ChangeVideoBackend(backend) => {
                let mut state = self.state.lock();
                state.video_backend = backend;
            }
            GuiCommand::ChangeAudioBackend(backend) => {
                let mut state = self.state.lock();
                state.audio_backend = backend;
            }
            GuiCommand::TogglePlayPause => {
                // Check if video is loaded
                let can_toggle = {
                    let state = self.state.lock();
                    state.is_video_loaded()
                };
                if !can_toggle {
                    eprintln!("Cannot toggle play/pause: no video loaded");
                    return;
                }
                // Toggle pipeline state
                if let Some(gst_source) = self.gst_source.as_ref() {
                    let source = gst_source.lock();
                    let current_state = self.state.lock().playback_state;
                    match current_state {
                        PlaybackState::Playing => {
                            source
                                .pause()
                                .unwrap_or_else(|e| eprintln!("Pause failed: {}", e));
                            self.state.lock().playback_state = PlaybackState::Paused;
                        }
                        PlaybackState::Paused => {
                            source
                                .resume()
                                .unwrap_or_else(|e| eprintln!("Resume failed: {}", e));
                            self.state.lock().playback_state = PlaybackState::Playing;
                        }
                    }
                }
            }
            GuiCommand::Seek(delta) => {
                if let Some(gst_source) = self.gst_source.as_ref() {
                    let mut source = gst_source.lock();
                    let _ = source.seek(delta.to_i64());
                }
                if let Some(pipeline) = self.pipeline.as_ref() {
                    let _ = pipeline.send_command(PipelineCommand::Seek(delta));
                }
            }
            GuiCommand::VolumeUp(step) => {
                let mut state = self.state.lock();
                state.volume.step_up(step);
                self.volume_atomic
                    .store(state.volume.get(), Ordering::Release);
            }
            GuiCommand::VolumeDown(step) => {
                let mut state = self.state.lock();
                state.volume.step_down(step);
                self.volume_atomic
                    .store(state.volume.get(), Ordering::Release);
            }
            GuiCommand::ToggleLogs => {
                let mut state = self.state.lock();
                state.show_logs = !state.show_logs;
            }
            GuiCommand::RestartPipeline => {
                self.restart_pipeline();
            }
            GuiCommand::ConfirmSetup => {
                // Ensure a video is selected
                let has_video = { self.state.lock().video_path.is_some() };
                if !has_video {
                    eprintln!("Cannot start playback: no video selected");
                    return;
                }
                // Set mode to Playback and build the pipeline
                {
                    let mut state = self.state.lock();
                    state.mode = AppMode::Playback;
                }
                self.restart_pipeline();
            }
            GuiCommand::BackToSetup => {
                // Stop pipeline, reset mode to Setup, clear last frame
                self.stop_pipeline();
                {
                    let mut state = self.state.lock();
                    state.mode = AppMode::Setup;
                    state.current_time_ns = 0;
                    state.total_duration_ns = 0;
                    state.playback_state = PlaybackState::Paused;
                }
                self.last_frame = None;
            }
        }
    }

    fn stop_pipeline(&mut self) {
        self.pipeline = None;
        self.gst_source = None;
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
        self.last_frame = None;
        self.audio_clock.reset();
        self.audio_clear_requested.store(true, Ordering::SeqCst);
    }

    fn restart_pipeline(&mut self) {
        // 1. Stop any existing pipeline
        self.stop_pipeline();

        // 2. Check mode and get values
        let (video_path_opt, audio_model_path_opt, video_backend, audio_backend, mode) = {
            let state = self.state.lock();
            (
                state.video_path.clone(),
                state.audio_model_path.clone(),
                state.video_backend,
                state.audio_backend,
                state.mode,
            )
        };

        if mode != AppMode::Playback {
            return;
        }

        // 3. Use default video if none selected
        let video_path = video_path_opt.unwrap_or_else(|| {
            let default_path = format!(
                "{}/../hfm-core/assets/video_with_music.mp4",
                env!("CARGO_MANIFEST_DIR")
            );
            PathBuf::from(default_path)
        });

        // 4. Build GStreamer source
        let gst_source = Arc::new(Mutex::new(
            GstSource::new(&video_path.to_string_lossy())
                .expect("failed to create GStreamer source"),
        ));
        self.gst_source = Some(gst_source.clone());

        // 5. Query duration and update state
        let duration = gst_source.lock().duration_ns().unwrap_or(0);
        self.state.lock().total_duration_ns = duration;

        // 6. Spawn video pump and pipeline controller
        let (video_tx, video_rx) = bounded(4);
        let video_pump = spawn_video_pump(gst_source.clone(), video_tx, self.generation.clone());
        self.video_pump = Some(video_pump);

        let video_source = ChannelVideoSource {
            rx: video_rx,
            generation: self.generation.clone(),
        };

        // 7. Build model with default if not provided, handle errors
        let model_path = audio_model_path_opt
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                format!(
                    "{}/../hfm-core/models/pphumanseg.onnx",
                    env!("CARGO_MANIFEST_DIR")
                )
            });

        let model = match PeopleSegFilter::new(&model_path) {
            Ok(m) => m,
            Err(e) => {
                let err_msg = format!("Failed to load video model: {}", e);
                eprintln!("{}", err_msg);
                // Push error to log panel
                self.state.lock().log_lines.push(err_msg);
                // Stop pipeline and switch back to setup
                self.stop_pipeline();
                self.state.lock().mode = AppMode::Setup;
                self.state.lock().playback_state = PlaybackState::Paused;
                return;
            }
        };

        let mut pipeline =
            PipelineController::new(Box::new(video_source), model, self.generation.clone());
        pipeline.start();
        self.pipeline = Some(pipeline);

        // 8. Spawn audio pipeline if an audio model is provided
        if let Some(audio_config) = self.build_audio_config(audio_model_path_opt, audio_backend) {
            let (raw_audio_tx, raw_audio_rx) = bounded(128);
            let audio_pump =
                spawn_audio_pump(gst_source.clone(), raw_audio_tx, self.generation.clone());
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
            );
            self.audio_output = Some(audio_output);
            self.has_audio = true;
        } else {
            self.has_audio = false;
            self.buffering.set(false);
        }

        // 9. Set initial playback state to Paused
        self.state.lock().playback_state = PlaybackState::Paused;
    }

    fn build_audio_config(
        &self,
        model_path: Option<PathBuf>,
        backend: Backend,
    ) -> Option<DemucsConfig> {
        model_path.map(|path| {
            let backend_str = match backend {
                Backend::Cpu => "cpu".to_string(),
                Backend::DirectML => "dml".to_string(),
                Backend::OpenVINO => "openvino".to_string(),
                Backend::CoreML => "coreml".to_string(),
            };
            DemucsConfig {
                model_path: path.to_string_lossy().to_string(),
                backend: backend_str,
                window_size: WINDOW_SAMPLES,
            }
        })
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("Audio+Video Pipeline")
                        .with_inner_size(winit::dpi::LogicalSize::new(960, 540)),
                )
                .unwrap(),
        );
        self.window = Some(window.clone());

        let renderer = pollster::block_on(Renderer::new(window));
        self.renderer = Some(renderer);
        // Set initial state
        {
            let mut state = self.state.lock();
            state.mode = AppMode::Setup;
            // Optionally, load CLI args into state (video_path, etc.)
        }
        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.handle_window_event(&event);
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::RedrawRequested => {
                let renderer = self.renderer.as_mut().unwrap();
                let state = self.state.clone();
                let bridge = &self.bridge;

                if self.buffering.is_buffering() {
                    renderer.render(self.last_frame.clone(), state, bridge);
                } else if let Some(pipeline) = self.pipeline.as_ref() {
                    if let Some(front_pts) = pipeline.peek_video_pts() {
                        if self.audio_clock.is_initialized() {
                            let now = self.audio_clock.now_ns();
                            if front_pts > now {
                                self.window.as_ref().unwrap().request_redraw();
                                return;
                            }
                        }

                        if let Some(frame) = pipeline.pop_processed_frame() {
                            let data = frame.data;

                            renderer.render(Some(data.clone()), state, bridge);
                            self.last_frame = Some(data);

                            self.frame_count += 1;
                            if self.fps_timer.elapsed() >= Duration::from_secs(1) {
                                println!("Video FPS: {}", self.frame_count);
                                self.frame_count = 0;
                                self.fps_timer = Instant::now();
                            }
                        }
                    } else {
                        renderer.render(self.last_frame.clone(), state, bridge);
                    }
                }

                self.window.as_ref().unwrap().request_redraw();
            }

            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
            }

            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        logical_key: winit::keyboard::Key::Named(named_key),
                        state: winit::event::ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                let delta = match named_key {
                    winit::keyboard::NamedKey::ArrowLeft => {
                        Some(SeekDelta::Backward(SEEK_DELTA_NS as u64))
                    }
                    winit::keyboard::NamedKey::ArrowRight => {
                        Some(SeekDelta::Forward(SEEK_DELTA_NS as u64))
                    }
                    _ => None,
                };

                if let Some(delta) = delta {
                    let now = Instant::now();
                    if now - self.last_seek_time < Duration::from_millis(SEEK_DEBOUNCE_MS) {
                        // Ignore this seek if it's too soon after the last one.
                        return;
                    }
                    self.last_seek_time = now;

                    {
                        let mut source = self.gst_source.as_ref().unwrap().lock();

                        self.audio_clock.reset();

                        if self.has_audio {
                            self.buffering.set(true);
                            self.audio_clear_requested.store(true, Ordering::SeqCst);
                        }

                        let _ = source.seek(delta.to_i64());
                    }

                    if let Some(pipeline) = self.pipeline.as_ref() {
                        let _ = pipeline.send_command(PipelineCommand::Seek(delta));
                    }
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Process GUI commands
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            self.handle_gui_command(cmd);
        }

        // Request redraw
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
