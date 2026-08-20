mod audio_processor;
mod gst_source;
mod renderer;
mod sync;

use crate::audio_processor::AudioTestConfig;
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use gst_source::GstSource;
use hfm_core::ml::PeopleSegFilter;
use hfm_core::pipeline::{FrameSource, PipelineCommand, PipelineController, SeekDelta};
use renderer::Renderer;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use sync::AvSync;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

// Audio constants
const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 2;
const WINDOW_SAMPLES: usize = 343980;

/// Video frame source backed by a crossbeam channel.
struct ChannelSource {
    rx: Receiver<(Vec<u8>, u64)>,
    eos: bool,
}

impl ChannelSource {
    fn new(rx: Receiver<(Vec<u8>, u64)>) -> Self {
        Self { rx, eos: false }
    }
}

impl FrameSource for ChannelSource {
    fn pull_frame(&mut self) -> Option<(Vec<u8>, u64)> {
        if self.eos {
            return None;
        }
        match self.rx.recv() {
            Ok(frame) => Some(frame),
            Err(_) => {
                self.eos = true;
                None
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

struct AudioPipelineHandles {
    process_thread: thread::JoinHandle<()>,
    output_stream: cpal::Stream,
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pipeline: Option<PipelineController>,
    demux_pump_thread: Option<thread::JoinHandle<()>>,
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
            demux_pump_thread: None,
            audio_pipeline: None,
            frame_count: 0,
            fps_timer: Instant::now(),
            av_sync: None,
        }
    }
}

impl App {
    fn spawn_demux_pump(
        gst_source: GstSource,
        video_tx: Sender<(Vec<u8>, u64)>,
        audio_tx: Option<Sender<(Vec<f32>, u64)>>,
        av_sync: Option<Arc<AvSync>>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let mut video_eos = false;
            let mut audio_eos = false;
            let poll_timeout = Duration::from_millis(5);

            while !(video_eos && audio_eos) {
                if !video_eos {
                    match gst_source.try_pull_video_frame(poll_timeout) {
                        Some((data, pts)) => {
                            if video_tx.send((data, pts)).is_err() {
                                break;
                            }
                        }
                        None if gst_source.is_video_eos() => {
                            video_eos = true;
                            println!("[PUMP] Video EOS");
                            if let Some(sync) = av_sync.as_ref() {
                                sync.set_video_ended();
                            }
                        }
                        _ => {}
                    }
                }

                if let Some(audio_tx) = audio_tx.as_ref() {
                    if !audio_eos {
                        match gst_source.try_pull_audio_frame(poll_timeout) {
                            Some((samples, pts)) => {
                                if audio_tx.send((samples, pts)).is_err() {
                                    break;
                                }
                            }
                            None if gst_source.is_audio_eos() => {
                                audio_eos = true;
                                println!("[PUMP] Audio EOS");
                            }
                            _ => {}
                        }
                    }
                } else {
                    audio_eos = true;
                }

                std::thread::yield_now();
            }

            println!("[PUMP] Demux pump finished");
        })
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("Audio+Video Pipeline (No-Drop, Synced)")
                        .with_inner_size(winit::dpi::LogicalSize::new(960, 540)),
                )
                .unwrap(),
        );
        self.window = Some(window.clone());

        let renderer = pollster::block_on(Renderer::new(window));
        self.renderer = Some(renderer);

        let audio_config = parse_audio_args();
        let av_sync_opt = if audio_config.is_some() {
            Some(Arc::new(AvSync::new(SAMPLE_RATE, CHANNELS, 200)))
        } else {
            None
        };
        self.av_sync = av_sync_opt.clone();

        let gst_source = GstSource::new().expect("Failed to create GStreamer source");

        let (video_tx, video_rx) = crossbeam_channel::bounded::<(Vec<u8>, u64)>(4);
        let (audio_tx, audio_rx) = if audio_config.is_some() {
            let (tx, rx) = crossbeam_channel::bounded::<(Vec<f32>, u64)>(128);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let pump_handle =
            Self::spawn_demux_pump(gst_source, video_tx, audio_tx, av_sync_opt.clone());
        self.demux_pump_thread = Some(pump_handle);

        let video_source = ChannelSource::new(video_rx);
        let model = PeopleSegFilter::new("models/pphumanseg.onnx")
            .expect("Failed to load PPHumanSeg model");
        let mut pipeline = PipelineController::new(Box::new(video_source), model);
        pipeline.start();
        self.pipeline = Some(pipeline);

        if let (Some(config), Some(audio_rx)) = (audio_config, audio_rx) {
            let av_sync = self.av_sync.clone().expect("audio sync must exist");
            match audio_processor::start_audio_pipeline(config, audio_rx, av_sync) {
                Ok(handles) => {
                    self.audio_pipeline = Some(AudioPipelineHandles {
                        process_thread: handles.process_thread,
                        output_stream: handles.output_stream,
                    });
                }
                Err(e) => {
                    eprintln!("Failed to start audio pipeline: {e}");
                }
            }
        }

        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::RedrawRequested => {
                let renderer = self.renderer.as_mut().unwrap();
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
                            let _ = pipeline.send_command(PipelineCommand::Seek(
                                SeekDelta::Backward(SEEK_DELTA_NS as u64),
                            ));
                        }
                        winit::keyboard::NamedKey::ArrowRight => {
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
