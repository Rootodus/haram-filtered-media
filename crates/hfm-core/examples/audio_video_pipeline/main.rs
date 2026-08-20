mod audio_processor;
mod gst_source;
mod renderer;
mod sync;

use crate::audio_processor::AudioTestConfig;
use anyhow::Result;
use gst_source::GstSource;
use hfm_core::ml::PeopleSegFilter;
use hfm_core::pipeline::{FrameSource, PipelineCommand, PipelineController, SeekDelta};
use renderer::Renderer;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use sync::{AvSync, PlaybackState};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 2;
const WINDOW_SAMPLES: usize = 343_980;

/// Wrapper around `GstSource` that provides thread‑safe access via a mutex.
/// Implements `FrameSource` so the video pipeline can use it directly.
struct SharedGstSource {
    inner: Arc<Mutex<GstSource>>,
    av_sync: Option<Arc<AvSync>>,
}

impl SharedGstSource {
    fn new(inner: Arc<Mutex<GstSource>>, av_sync: Option<Arc<AvSync>>) -> Self {
        Self { inner, av_sync }
    }
}

impl FrameSource for SharedGstSource {
    fn pull_frame(&mut self) -> Option<(Vec<u8>, u64)> {
        loop {
            let source = self.inner.lock().unwrap();
            match source.try_pull_video_frame(Duration::from_millis(5)) {
                Some(frame) => return Some(frame),
                None if source.is_video_eos() => return None,
                None => {
                    drop(source);
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    fn seek(&mut self, delta_ns: i64) -> Result<(), String> {
        let result = self.inner.lock().unwrap().seek(delta_ns);
        if let Some(av_sync) = self.av_sync.as_ref() {
            av_sync.reset_after_seek();
        }
        result
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

struct AudioPipelineHandles {
    process_thread: thread::JoinHandle<()>,
    cpal_thread: thread::JoinHandle<()>,
    pull_thread: thread::JoinHandle<()>,
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pipeline: Option<PipelineController>,
    audio_pipeline: Option<AudioPipelineHandles>,
    frame_count: u32,
    fps_timer: Instant,
    av_sync: Option<Arc<AvSync>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            pipeline: None,
            audio_pipeline: None,
            frame_count: 0,
            fps_timer: Instant::now(),
            av_sync: None,
        }
    }
}

impl App {
    fn is_playing(&self) -> bool {
        match self.av_sync.as_ref() {
            Some(sync) => sync.get_state() == PlaybackState::Playing,
            None => true, // video-only always plays
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("Audio+Video Pipeline (Buffered Sync)")
                        .with_inner_size(winit::dpi::LogicalSize::new(960, 540)),
                )
                .unwrap(),
        );
        self.window = Some(window.clone());

        let renderer = pollster::block_on(Renderer::new(window));
        self.renderer = Some(renderer);

        let audio_config = parse_audio_args();
        let av_sync = if audio_config.is_some() {
            Some(Arc::new(AvSync::new(SAMPLE_RATE, 200)))
        } else {
            None
        };
        self.av_sync = av_sync.clone();

        // Create a single shared GStreamer source.
        let gst_source = Arc::new(Mutex::new(
            GstSource::new().expect("Failed to create GStreamer source"),
        ));

        // Video pipeline using the shared source.
        let video_source = SharedGstSource::new(gst_source.clone(), av_sync.clone());
        let model =
            PeopleSegFilter::new("models/pphumanseg.onnx").expect("Failed to load PPHumanSeg");
        let mut pipeline = PipelineController::new(Box::new(video_source), model);
        pipeline.start();
        self.pipeline = Some(pipeline);

        // Audio path: if audio model is provided, spawn a pull thread that
        // reads audio from the shared source and sends it to the audio
        // processor. The processor runs asynchronously and will start CPAL
        // when ready.
        if let Some(config) = audio_config {
            let (audio_tx, audio_rx) = crossbeam_channel::bounded::<(Vec<f32>, u64)>(128);

            let audio_source = gst_source.clone();
            let av_sync_pull = av_sync.clone().expect("audio sync must exist");

            let pull_handle = thread::spawn(move || {
                loop {
                    // If seeking, skip pulling/sending to avoid stale chunks.
                    if av_sync_pull.get_state() == PlaybackState::Seeking {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }

                    let source = audio_source.lock().unwrap();
                    match source.try_pull_audio_frame(Duration::from_millis(5)) {
                        Some((samples, pts)) => {
                            drop(source);
                            if audio_tx.send((samples, pts)).is_err() {
                                break;
                            }
                        }
                        None if source.is_audio_eos() => break,
                        None => {
                            drop(source);
                            std::thread::sleep(Duration::from_millis(1));
                        }
                    }
                }
                println!("[AUDIO_PULL] finished");
            });

            match audio_processor::start_audio_pipeline(config, audio_rx, av_sync.unwrap()) {
                Ok(handles) => {
                    self.audio_pipeline = Some(AudioPipelineHandles {
                        process_thread: handles.process_thread,
                        cpal_thread: handles.cpal_thread,
                        pull_thread: pull_handle,
                    });
                }
                Err(e) => {
                    eprintln!("Failed to start audio pipeline: {e}");
                }
            }
        }

        // If no audio, we are always playing.
        if self.av_sync.is_none() {
            // No state needed; video-only.
        } else {
            // Initially buffering until audio ready.
            self.av_sync
                .as_ref()
                .unwrap()
                .set_state(PlaybackState::Buffering);
        }

        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::RedrawRequested => {
                let playing = match self.av_sync.as_ref() {
                    Some(sync) => sync.get_state() == PlaybackState::Playing,
                    None => true,
                };

                let renderer = self.renderer.as_mut().unwrap();

                if !playing {
                    renderer.render(None);
                } else {
                    let frame = self.pipeline.as_ref().unwrap().pop_processed_frame();
                    if let Some(frame) = frame {
                        let pts = frame.pts.0;
                        if let Some(av_sync) = self.av_sync.as_ref() {
                            av_sync.wait_video(pts);
                        }
                        renderer.render(Some(frame.data));
                        if let Some(av_sync) = self.av_sync.as_ref() {
                            av_sync.report_video_pts(pts);
                        }
                    } else {
                        renderer.render(None);
                    }
                }

                self.frame_count += 1;
                if self.fps_timer.elapsed() >= Duration::from_secs(1) {
                    println!("Video FPS: {}", self.frame_count);
                    self.frame_count = 0;
                    self.fps_timer = Instant::now();
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
                const SEEK_DELTA_NS: i64 = 10_000_000_000;

                if let Some(pipeline) = self.pipeline.as_ref() {
                    match named_key {
                        winit::keyboard::NamedKey::ArrowLeft => {
                            if let Some(av_sync) = self.av_sync.as_ref() {
                                av_sync.set_state(PlaybackState::Seeking);
                            }
                            let _ = pipeline.send_command(PipelineCommand::Seek(
                                SeekDelta::Backward(SEEK_DELTA_NS as u64),
                            ));
                        }
                        winit::keyboard::NamedKey::ArrowRight => {
                            if let Some(av_sync) = self.av_sync.as_ref() {
                                av_sync.set_state(PlaybackState::Seeking);
                            }
                            let _ = pipeline.send_command(PipelineCommand::Seek(
                                SeekDelta::Forward(SEEK_DELTA_NS as u64),
                            ));
                        }
                        _ => {}
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
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
