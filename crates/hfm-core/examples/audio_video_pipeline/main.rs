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
use hfm_core::pipeline::{FrameSource, PipelineCommand, PipelineController, SeekDelta};
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

/// Adapter that turns `Receiver<RawVideoFrame>` into `hfm_core::FrameSource`.
struct ChannelVideoSource {
    rx: Receiver<RawVideoFrame>,
    generation: Arc<SeekGeneration>,
}

impl FrameSource for ChannelVideoSource {
    fn pull_frame(&mut self) -> Option<(Vec<u8>, u64)> {
        loop {
            match self.rx.recv() {
                Ok(frame) => {
                    if frame.generation == self.generation.current() {
                        return Some((frame.data, frame.pts_ns));
                    }
                }
                Err(_) => return None,
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
            let maybe_frame = {
                let source = gst_source.lock().unwrap();
                source.try_pull_video_frame(Duration::from_millis(5))
            };

            match maybe_frame {
                Some((data, pts_ns)) => {
                    let msg = RawVideoFrame {
                        data,
                        pts_ns,
                        generation: generation.current(),
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
            let maybe_chunk = {
                let source = gst_source.lock().unwrap();
                source.try_pull_audio_frame(Duration::from_millis(5))
            };

            match maybe_chunk {
                Some((samples, pts_ns)) => {
                    let msg = RawAudioChunk {
                        samples,
                        pts_ns,
                        generation: generation.current(),
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

    frame_count: u32,
    fps_timer: Instant,
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
            frame_count: 0,
            fps_timer: Instant::now(),
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
        if audio_config.is_none() {
            // Video-only mode: no audio output will ever clear the buffering flag.
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
                    renderer.render(None);
                } else if let Some(pipeline) = self.pipeline.as_ref() {
                    if let Some(frame) = pipeline.pop_processed_frame() {
                        if self.audio_clock.is_initialized() {
                            let now = self.audio_clock.now_ns();
                            if frame.pts.0 > now {
                                self.window.as_ref().unwrap().request_redraw();
                                return;
                            }
                        }
                        renderer.render(Some(frame.data));

                        self.frame_count += 1;
                        if self.fps_timer.elapsed() >= Duration::from_secs(1) {
                            println!("Video FPS: {}", self.frame_count);
                            self.frame_count = 0;
                            self.fps_timer = Instant::now();
                        }
                    }
                    // If no new frame is available, do not render black.
                    // This prevents flicker between video frames.
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
                    self.generation.increment();
                    self.audio_clock.reset();
                    self.buffering.set(true);

                    if let Some(source) = self.gst_source.as_ref() {
                        let mut source = source.lock().unwrap();
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
