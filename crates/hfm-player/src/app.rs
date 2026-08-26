//! The main application state and event loop.

use crate::config::*;
use crate::gui::{AppMode, AppState, Bridge, GuiCommand, PlaybackState};
use crate::pipeline_manager::PipelineManager;
use crate::pts_offset::SyncState;
use crate::renderer::Renderer;
use crossbeam_channel::{Receiver, unbounded};
use hfm_core::coordination::{AudioClock, BufferingFlag, SeekGeneration};
use hfm_core::pipeline::SeekDelta;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

pub struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pipeline_manager: PipelineManager,

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
    sync_state: SyncState,
    volume_atomic: Arc<AtomicU8>,
    is_playing: Arc<AtomicBool>,
}

impl App {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = unbounded();
        let bridge = Bridge::new(cmd_tx);
        let state = Arc::new(Mutex::new(AppState::default()));
        let volume_atomic = Arc::new(AtomicU8::new(state.lock().volume.get()));
        let generation = Arc::new(SeekGeneration::new());
        let audio_clock = Arc::new(AudioClock::new());
        let buffering = Arc::new(BufferingFlag::new(true));
        let audio_clear_requested = Arc::new(AtomicBool::new(false));
        let is_playing = Arc::new(AtomicBool::new(false));
        let pipeline_manager = PipelineManager::new(
            generation.clone(),
            audio_clock.clone(),
            buffering.clone(),
            audio_clear_requested.clone(),
            volume_atomic.clone(),
            is_playing.clone(),
        );

        Self {
            window: None,
            renderer: None,
            pipeline_manager,
            generation,
            audio_clock,
            buffering,
            audio_clear_requested,
            has_audio: false,
            frame_count: 0,
            fps_timer: Instant::now(),
            last_frame: None,
            last_seek_time: Instant::now(),
            state,
            bridge,
            cmd_rx,
            sync_state: SyncState::new(),
            volume_atomic,
            is_playing,
        }
    }

    fn handle_gui_command(&mut self, cmd: GuiCommand) {
        match cmd {
            GuiCommand::LoadVideo(path) => {
                self.state.lock().video_path = Some(path);
                // Don't restart automatically – wait for ConfirmSetup.
            }
            GuiCommand::ToggleVideoFilter => {
                let mut state = self.state.lock();
                state.video_filter_enabled = !state.video_filter_enabled;
            }
            GuiCommand::ToggleAudioProcessing => {
                let mut state = self.state.lock();
                state.audio_processing_enabled = !state.audio_processing_enabled;
            }
            GuiCommand::ChangeVideoBackend(backend) => {
                self.state.lock().video_backend = backend;
            }
            GuiCommand::ChangeAudioBackend(backend) => {
                self.state.lock().audio_backend = backend;
            }
            GuiCommand::TogglePlayPause => {
                let can_toggle = self.state.lock().is_video_loaded();
                if !can_toggle {
                    eprintln!("Cannot toggle play/pause: no video loaded");
                    return;
                }
                self.sync_state.reset();

                let current_state = self.state.lock().playback_state;
                match current_state {
                    PlaybackState::Playing => {
                        if let Err(e) = self.pipeline_manager.pause_playback() {
                            eprintln!("Pause failed: {}", e);
                        } else {
                            self.state.lock().playback_state = PlaybackState::Paused;
                        }
                    }
                    PlaybackState::Paused => {
                        if let Err(e) = self.pipeline_manager.resume_playback() {
                            eprintln!("Resume failed: {}", e);
                        } else {
                            self.state.lock().playback_state = PlaybackState::Playing;
                        }
                    }
                }
            }
            GuiCommand::Seek(delta) => {
                self.sync_state.reset();
                if let Err(e) = self.pipeline_manager.seek(delta) {
                    eprintln!("Seek failed: {}", e);
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
            GuiCommand::ConfirmSetup => {
                self.state.lock().mode = AppMode::Playback;
                self.sync_state.reset();

                let (video_path, video_backend, audio_backend, filter_enabled, audio_enabled) = {
                    let state = self.state.lock();
                    (
                        state.video_path.clone().unwrap_or_else(|| {
                            let default_path = format!(
                                "{}/../hfm-core/assets/video_with_music.mp4",
                                env!("CARGO_MANIFEST_DIR")
                            );
                            PathBuf::from(default_path)
                        }),
                        state.video_backend,
                        state.audio_backend,
                        state.video_filter_enabled,
                        state.audio_processing_enabled,
                    )
                };

                match self.pipeline_manager.restart(
                    video_path,
                    video_backend,
                    audio_backend,
                    filter_enabled,
                    audio_enabled,
                ) {
                    Ok(duration) => {
                        self.state.lock().total_duration_ns = duration;
                        self.has_audio = self.pipeline_manager.has_audio;
                        self.state.lock().playback_state = PlaybackState::Paused;
                    }
                    Err(e) => {
                        eprintln!("Restart failed: {}", e);
                        self.state.lock().log_lines.push(e);
                        self.state.lock().mode = AppMode::Setup;
                        self.state.lock().playback_state = PlaybackState::Paused;
                    }
                }
            }
            GuiCommand::BackToSetup => {
                self.pipeline_manager.stop();
                let mut state = self.state.lock();
                state.mode = AppMode::Setup;
                state.current_time_ns = 0;
                state.total_duration_ns = 0;
                state.playback_state = PlaybackState::Paused;
                self.sync_state.reset();
                self.last_frame = None;
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("hfm‑player")
                        .with_inner_size(winit::dpi::LogicalSize::new(960, 540)),
                )
                .unwrap(),
        );
        self.window = Some(window.clone());

        let renderer = pollster::block_on(Renderer::new(window));
        self.renderer = Some(renderer);

        // Start in Setup mode
        self.state.lock().mode = AppMode::Setup;
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

                // Update current time from audio clock for UI
                {
                    let mut state = state.lock();
                    state.current_time_ns = self.audio_clock.now_ns();
                }

                if self.pipeline_manager.is_buffering() {
                    renderer.render(self.last_frame.clone(), state, bridge);
                } else if let Some(front_pts) = self.pipeline_manager.peek_video_pts() {
                    let now = self.audio_clock.now_ns();
                    let adjusted_pts = self.sync_state.adjust_pts(front_pts, now);
                    let delta = adjusted_pts - now as i64;

                    if self.audio_clock.is_initialized() {
                        if delta > SYNC_TOLERANCE_NS {
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                    }

                    if let Some(frame) = self.pipeline_manager.pop_processed_frame() {
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
                        return;
                    }
                    self.last_seek_time = now;
                    self.sync_state.reset();

                    self.audio_clock.reset();
                    if self.has_audio {
                        self.buffering.set(true);
                        self.audio_clear_requested.store(true, Ordering::SeqCst);
                    }

                    if let Err(e) = self.pipeline_manager.seek(delta) {
                        eprintln!("Seek failed: {}", e);
                    }
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            // Reset sync state on seek commands
            if let GuiCommand::Seek(_) = cmd {
                self.sync_state.reset();
            }
            self.handle_gui_command(cmd);
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
