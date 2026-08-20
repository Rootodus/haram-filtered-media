//! Composition root for the audio/video pipeline example.
//!
//! This file only wires modules together. It contains no GStreamer,
//! ONNX, CPAL, wgpu, or processing-loop logic itself.

mod audio_output;
mod audio_processor;
mod gst_source;
mod renderer;
mod sync;
mod types;

use crate::audio_output::spawn_audio_output;
use crate::audio_processor::spawn_audio_processor;
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

/// Adapter that turns `Receiver<RawVideoFrame>` into `hfm_core::FrameSource`.
///
/// It drops stale frames by checking the shared seek generation.
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
                    // Stale frame; discard and wait for next one.
                }
                Err(_) => return None,
            }
        }
    }

    fn seek(&mut self, _delta_ns: i64) -> Result<(), String> {
        // The real GStreamer seek is handled by the composition root.
        Ok(())
    }
}

fn spawn_source_pump(
    gst_source: Arc<Mutex<GstSource>>,
    video_tx: Sender<RawVideoFrame>,
    audio_tx: Option<Sender<RawAudioChunk>>,
    generation: Arc<SeekGeneration>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        loop {
            // Pull and send one video frame, if available.
            {
                let source = gst_source.lock().unwrap();
                if let Some(frame) = source.try_pull_video_frame() {
                    let current_gen = generation.current();
                    let msg = RawVideoFrame {
                        data: frame.data,
                        pts_ns: frame.pts_ns,
                        generation: current_gen,
                    };
                    if video_tx.send(msg).is_err() {
                        break;
                    }
                    continue;
                }
            }

            // If audio is enabled, pull and send one audio chunk.
            if let Some(audio_tx) = audio_tx.as_ref() {
                let source = gst_source.lock().unwrap();
                if let Some(chunk) = source.try_pull_audio_frame() {
                    let current_gen = generation.current();
                    let msg = RawAudioChunk {
                        samples: chunk.samples,
                        pts_ns: chunk.pts_ns,
                        generation: current_gen,
                    };
                    if audio_tx.send(msg).is_err() {
                        break;
                    }
                    continue;
                }
            }

            // No data available. Yield briefly.
            thread::sleep(Duration::from_millis(1));
        }
    })
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pipeline: Option<PipelineController>,

    gst_source: Option<Arc<Mutex<GstSource>>>,

    _source_pump: Option<thread::JoinHandle<()>>,
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
            _source_pump: None,
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

        let use_audio = std::env::args().any(|a| a == "--audio-model");

        let gst_source = Arc::new(Mutex::new(
            GstSource::new().expect("failed to create GStreamer source"),
        ));
        self.gst_source = Some(gst_source.clone());

        let (video_tx, video_rx) = bounded::<RawVideoFrame>(4);

        let audio_channel = if use_audio {
            let (tx, rx) = bounded::<RawAudioChunk>(128);
            Some((tx, rx))
        } else {
            None
        };

        // Spawn the source pump thread.
        let pump = spawn_source_pump(
            gst_source.clone(),
            video_tx,
            audio_channel.as_ref().map(|(tx, _)| tx.clone()),
            self.generation.clone(),
        );
        self._source_pump = Some(pump);

        // Start the video worker (existing hfm-core pipeline).
        let video_source = ChannelVideoSource {
            rx: video_rx,
            generation: self.generation.clone(),
        };
        let model =
            PeopleSegFilter::new("models/pphumanseg.onnx").expect("failed to load PPHumanSeg");
        let mut pipeline = PipelineController::new(Box::new(video_source), model);
        pipeline.start();
        self.pipeline = Some(pipeline);

        // Start the audio worker and output sink, if requested.
        if let Some((_, raw_audio_rx)) = audio_channel {
            let (processed_audio_tx, processed_audio_rx) = bounded::<ProcessedAudioChunk>(128);

            let audio_processor =
                spawn_audio_processor(raw_audio_rx, processed_audio_tx, self.generation.clone());
            self._audio_processor = Some(audio_processor);

            let audio_output = spawn_audio_output(
                processed_audio_rx,
                self.audio_clock.clone(),
                self.buffering.clone(),
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

                // Buffering/black screen has priority.
                if self.buffering.is_buffering() {
                    renderer.render(None);
                } else if let Some(pipeline) = self.pipeline.as_ref() {
                    match pipeline.pop_processed_frame() {
                        Some(frame) => {
                            // TODO: use audio clock PTS pacing once implemented.
                            renderer.render(Some(frame.data));
                        }
                        None => {
                            renderer.render(None);
                        }
                    }
                } else {
                    renderer.render(None);
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
                    // 1. Invalidate all stale work.
                    self.generation.increment();

                    // 2. Reset playback timing state.
                    self.audio_clock.reset();
                    self.buffering.set(true);

                    // 3. Seek the real GStreamer source.
                    if let Some(source) = self.gst_source.as_ref() {
                        let mut source = source.lock().unwrap();
                        let _ = source.seek(delta.to_i64());
                    }

                    // 4. Flush the video pipeline's internal buffers.
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
