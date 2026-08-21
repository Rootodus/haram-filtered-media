mod audio_output;
mod audio_processor;
mod gst_source;
mod renderer;
mod sync;
mod types;

use crate::audio_output::spawn_audio_output;
use crate::audio_processor::{AudioTestConfig, spawn_audio_processor};
use crate::gst_source::GstSource;
use crate::renderer::Renderer;
use crate::sync::{AudioClock, BufferingFlag, SeekGeneration};
use crate::types::{ProcessedAudioChunk, RawAudioChunk, RawVideoFrame};
use crossbeam_channel::{Receiver, Sender, bounded};
use hfm_core::ml::PeopleSegFilter;
use hfm_core::pipeline::{
    FrameSource, PipelineCommand, PipelineController, PullOutcome, SeekDelta,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

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

fn parse_audio_args() -> Option<AudioTestConfig> {
    let args: Vec<String> = std::env::args().collect();
    let mut model_path = None;
    let mut backend = "cpu".to_string();
    let mut window_size = WINDOW_SAMPLES;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--audio-model" => {
                model_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--audio-backend" => {
                backend = args[i + 1].clone();
                i += 2;
            }
            "--audio-window" => {
                window_size = args[i + 1].parse().unwrap_or(WINDOW_SAMPLES);
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    model_path.map(|path| AudioTestConfig {
        model_path: path,
        backend,
        window_size,
    })
}

fn spawn_video_pump(
    gst_source: Arc<Mutex<GstSource>>,
    video_tx: Sender<RawVideoFrame>,
    generation: Arc<SeekGeneration>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        loop {
            let (maybe_frame, current_gen) = {
                let source = gst_source.lock().unwrap();
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
                    let source = gst_source.lock().unwrap();
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
}

fn spawn_audio_pump(
    gst_source: Arc<Mutex<GstSource>>,
    audio_tx: Sender<RawAudioChunk>,
    generation: Arc<SeekGeneration>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        loop {
            let (maybe_chunk, current_gen) = {
                let source = gst_source.lock().unwrap();
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
                    let source = gst_source.lock().unwrap();
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
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pipeline: Option<PipelineController>,

    gst_source: Option<Arc<Mutex<GstSource>>>,

    _video_pump: Option<thread::JoinHandle<()>>,
    _audio_pump: Option<thread::JoinHandle<()>>,
    _audio_processor: Option<thread::JoinHandle<()>>,
    _audio_output: Option<thread::JoinHandle<()>>,

    generation: Arc<SeekGeneration>,
    audio_clock: Arc<AudioClock>,
    buffering: Arc<BufferingFlag>,
    audio_clear_requested: Arc<AtomicBool>,

    has_audio: bool,
    frame_count: u32,
    fps_timer: Instant,
    last_frame: Option<Vec<u8>>,
    last_seek_time: Instant,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            pipeline: None,
            gst_source: None,
            _video_pump: None,
            _audio_pump: None,
            _audio_processor: None,
            _audio_output: None,
            generation: Arc::new(SeekGeneration::new()),
            audio_clock: Arc::new(AudioClock::new(SAMPLE_RATE)),
            buffering: Arc::new(BufferingFlag::new(true)),
            audio_clear_requested: Arc::new(AtomicBool::new(false)),
            has_audio: false,
            frame_count: 0,
            fps_timer: Instant::now(),
            last_frame: None,
            last_seek_time: Instant::now(),
        }
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

        let audio_config = parse_audio_args();
        self.has_audio = audio_config.is_some();

        if !self.has_audio {
            self.buffering.set(false);
        }

        let gst_source = Arc::new(Mutex::new(
            GstSource::new().expect("failed to create GStreamer source"),
        ));
        self.gst_source = Some(gst_source.clone());

        let (video_tx, video_rx) = bounded::<RawVideoFrame>(4);

        let video_pump = spawn_video_pump(gst_source.clone(), video_tx, self.generation.clone());
        self._video_pump = Some(video_pump);

        let video_source = ChannelVideoSource {
            rx: video_rx,
            generation: self.generation.clone(),
        };
        let model =
            PeopleSegFilter::new("models/pphumanseg.onnx").expect("failed to load PPHumanSeg");
        let mut pipeline = PipelineController::new(Box::new(video_source), model);
        pipeline.start();
        self.pipeline = Some(pipeline);

        if let Some(config) = audio_config {
            let (raw_audio_tx, raw_audio_rx) = bounded::<RawAudioChunk>(128);

            let audio_pump =
                spawn_audio_pump(gst_source.clone(), raw_audio_tx, self.generation.clone());
            self._audio_pump = Some(audio_pump);

            let (processed_audio_tx, processed_audio_rx) = bounded::<ProcessedAudioChunk>(128);

            let audio_processor = spawn_audio_processor(
                config,
                raw_audio_rx,
                processed_audio_tx,
                self.generation.clone(),
            );
            self._audio_processor = Some(audio_processor);

            let audio_output = spawn_audio_output(
                processed_audio_rx,
                self.audio_clock.clone(),
                self.buffering.clone(),
                self.generation.clone(),
                self.audio_clear_requested.clone(),
                SAMPLE_RATE,
                CHANNELS,
            );
            self._audio_output = Some(audio_output);
        }

        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::RedrawRequested => {
                let renderer = self.renderer.as_mut().unwrap();

                if self.buffering.is_buffering() {
                    renderer.render(self.last_frame.clone());
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

                            renderer.render(Some(data.clone()));
                            self.last_frame = Some(data);

                            self.frame_count += 1;
                            if self.fps_timer.elapsed() >= Duration::from_secs(1) {
                                println!("Video FPS: {}", self.frame_count);
                                self.frame_count = 0;
                                self.fps_timer = Instant::now();
                            }
                        }
                    } else {
                        renderer.render(self.last_frame.clone());
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

                    let generation_val;
                    {
                        let mut source = self.gst_source.as_ref().unwrap().lock().unwrap();

                        generation_val = self.generation.increment();
                        self.audio_clock.reset();

                        if self.has_audio {
                            self.buffering.set(true);
                            self.audio_clear_requested.store(true, Ordering::SeqCst);
                        }

                        let _ = source.seek(delta.to_i64());
                    }

                    if let Some(pipeline) = self.pipeline.as_ref() {
                        let _ = pipeline.send_command(PipelineCommand::Seek {
                            delta,
                            generation: generation_val,
                        });
                    }
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
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
