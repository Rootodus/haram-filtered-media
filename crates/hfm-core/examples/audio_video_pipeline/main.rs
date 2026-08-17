mod gst_source;
mod renderer;

use gst_source::GstSource;
use hfm_core::ml::PeopleSegFilter;
use hfm_core::pipeline::{PipelineCommand, PipelineController, SeekDelta};
use renderer::Renderer;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

// Audio imports
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Producer, Split};

// Audio constants
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const SPSC_CAPACITY: usize = 16384;

// Audio test module (unchanged – for dummy benchmark)
mod audio_test {
    use anyhow::{Result, anyhow};
    use ndarray::{Array, IxDyn};
    use ort::session::Session;
    use ort::session::builder::GraphOptimizationLevel;
    use ort::value::Value;
    use std::time::{Duration, Instant};

    /// Build a session with a specific backend (cpu or dml)
    fn build_session(path: &str, backend: &str) -> Result<Session> {
        match backend {
            "cpu" => {
                let mut builder = Session::builder()
                    .map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
                builder = builder
                    .with_optimization_level(GraphOptimizationLevel::Level1)
                    .map_err(|e| anyhow!("Failed to set optimization level: {}", e))?;
                builder = builder
                    .with_intra_threads(1)
                    .map_err(|e| anyhow!("Failed to set intra threads: {}", e))?;
                builder = builder
                    .with_execution_providers([ort::ep::CPUExecutionProvider::default().build()])
                    .map_err(|e| anyhow!("Failed to set CPU provider: {}", e))?;
                let session = builder
                    .commit_from_file(path)
                    .map_err(|e| anyhow!("Failed to load model on CPU: {}", e))?;
                println!("SUCCESS: CPU backend active.");
                Ok(session)
            }
            "dml" => {
                #[cfg(target_os = "windows")]
                {
                    use ort::ep::DirectMLExecutionProvider;

                    // Build DML session from scratch
                    let mut dml_builder = Session::builder()
                        .map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
                    dml_builder = dml_builder
                        .with_optimization_level(GraphOptimizationLevel::Level1)
                        .map_err(|e| anyhow!("Failed to set optimization level: {}", e))?;
                    dml_builder = dml_builder
                        .with_intra_threads(1)
                        .map_err(|e| anyhow!("Failed to set intra threads: {}", e))?;
                    dml_builder = dml_builder
                        .with_disable_cpu_fallback()
                        .map_err(|e| anyhow!("Failed to disable CPU fallback: {}", e))?;
                    dml_builder = dml_builder
                        .with_execution_providers([DirectMLExecutionProvider::default().build()])
                        .map_err(|e| anyhow!("Failed to set DirectML provider: {}", e))?;

                    match dml_builder.commit_from_file(path) {
                        Ok(session) => {
                            println!("SUCCESS: DirectML hardware backend is active.");
                            Ok(session)
                        }
                        Err(e) => {
                            println!("DirectML failed: {}. Falling back to CPU...", e);
                            // Build CPU session from scratch (do not reuse the moved builder)
                            let mut cpu_builder = Session::builder().map_err(|e| {
                                anyhow!("Failed to create CPU fallback builder: {}", e)
                            })?;
                            cpu_builder = cpu_builder
                                .with_optimization_level(GraphOptimizationLevel::Level1)
                                .map_err(|e| {
                                    anyhow!("Failed to set CPU optimization level: {}", e)
                                })?;
                            cpu_builder = cpu_builder
                                .with_intra_threads(1)
                                .map_err(|e| anyhow!("Failed to set CPU intra threads: {}", e))?;
                            cpu_builder = cpu_builder
                                .with_execution_providers([ort::ep::CPUExecutionProvider::default(
                                )
                                .build()])
                                .map_err(|e| {
                                    anyhow!("Failed to set CPU provider in fallback: {}", e)
                                })?;
                            let session = cpu_builder.commit_from_file(path).map_err(|e| {
                                anyhow!("Failed to load model on CPU fallback: {}", e)
                            })?;
                            println!("SUCCESS: CPU backend active (fallback).");
                            Ok(session)
                        }
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    Err(anyhow!("DirectML is only supported on Windows"))
                }
            }
            _ => Err(anyhow!("Unsupported backend: {}", backend)),
        }
    }

    pub fn run_audio_benchmark(
        model_path: String,
        backend: String,
        duration_secs: u64,
        shape: Vec<usize>,
    ) -> Result<()> {
        // session must be mutable because run() may require &mut self
        let mut session = build_session(&model_path, &backend)?;

        let total_elements: usize = shape.iter().product();
        let dummy_data = vec![0.0f32; total_elements];
        let input_shape = IxDyn(&shape);
        let input_array = Array::from_shape_vec(input_shape, dummy_data)?;
        let input_value = Value::from_array(input_array)?;

        // Warm-up
        for _ in 0..5 {
            let _ = session.run(ort::inputs![input_value.clone()])?;
        }

        let mut times = Vec::new();
        let start = Instant::now();

        while start.elapsed() < Duration::from_secs(duration_secs) {
            let t0 = Instant::now();
            let _ = session.run(ort::inputs![input_value.clone()])?;
            times.push(t0.elapsed());
        }

        let n = times.len();
        if n == 0 {
            return Ok(());
        }
        let mean_us = times.iter().map(|d| d.as_micros() as f64).sum::<f64>() / n as f64;
        let p95_us = {
            let mut sorted = times
                .iter()
                .map(|d| d.as_micros() as f64)
                .collect::<Vec<_>>();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            sorted[(n * 95 / 100).min(n - 1)]
        };
        let samples = shape.get(2).copied().unwrap_or(1024) as f64;
        let sample_rate = 16000.0;
        let window_dur = samples / sample_rate;
        let rtf = mean_us / 1_000_000.0 / window_dur;

        println!("\n--- Audio Benchmark Results ---");
        println!("Backend: {}", backend);
        println!("Iterations: {}", n);
        println!("Mean inference: {:.1} µs", mean_us);
        println!("p95: {:.1} µs", p95_us);
        println!("RTF: {:.3}", rtf);
        println!("Throughput: {:.1} inf/s", n as f64 / duration_secs as f64);
        if rtf < 0.1 {
            println!("✅ RTF < 0.1 – suitable for real-time.");
        } else {
            println!("❌ RTF >= 0.1 – may be too slow.");
        }

        Ok(())
    }
}

// Configuration struct
struct AudioTestConfig {
    model_path: String,
    backend: String,
    duration_secs: u64,
    shape: Vec<usize>,
}

// Parse command line arguments (simple manual parsing)
fn parse_args() -> Option<AudioTestConfig> {
    let args: Vec<String> = std::env::args().collect();
    let mut model_path = None;
    let mut backend = "cpu".to_string();
    let mut duration = 30;
    let mut shape = vec![1, 2, 1024];

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
            "--audio-duration" => {
                duration = args[i + 1].parse().unwrap_or(30);
                i += 2;
            }
            "--audio-shape" => {
                let parts: Vec<usize> =
                    args[i + 1].split(',').map(|s| s.parse().unwrap()).collect();
                if parts.len() >= 3 {
                    shape = parts;
                }
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
        duration_secs: duration,
        shape,
    })
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pipeline: Option<PipelineController>,
    audio_source: Option<GstSource>,
    audio_output_thread: Option<thread::JoinHandle<()>>,
    audio_stream: Option<cpal::Stream>, // keep the stream alive
    audio_test_thread: Option<thread::JoinHandle<()>>,
    frame_count: u32,
    fps_timer: Instant,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            pipeline: None,
            audio_source: None,
            audio_output_thread: None,
            audio_stream: None,
            audio_test_thread: None,
            frame_count: 0,
            fps_timer: Instant::now(),
        }
    }
}

impl App {
    fn init_pipeline(&mut self) {
        let source = GstSource::new().expect("Failed to create GStreamer source");
        let model = PeopleSegFilter::new("models/pphumanseg.onnx").expect("Failed to load model");
        let mut pipeline = PipelineController::new(Box::new(source), model);
        pipeline.start();
        self.pipeline = Some(pipeline);
    }

    fn start_audio_test(&mut self, config: AudioTestConfig) {
        let handle = thread::spawn(move || {
            if let Err(e) = audio_test::run_audio_benchmark(
                config.model_path,
                config.backend,
                config.duration_secs,
                config.shape,
            ) {
                eprintln!("Audio test error: {}", e);
            }
        });
        self.audio_test_thread = Some(handle);
    }

    fn start_audio_output(&mut self) {
        // Create a separate GstSource for audio
        let audio_source = GstSource::new().expect("Failed to create audio source");
        self.audio_source = Some(audio_source);

        // Ring buffer
        let producer = HeapRb::<f32>::new(SPSC_CAPACITY);
        let (mut producer, mut consumer) = producer.split();

        // Spawn thread to pull audio frames
        let mut audio_source = self.audio_source.take().unwrap();
        let output_handle = thread::spawn(move || {
            while let Some((samples, _pts)) = audio_source.pull_audio_frame() {
                let mut offset = 0;
                let total = samples.len();
                while offset < total {
                    let written = producer.push_slice(&samples[offset..]);
                    if written == 0 {
                        thread::sleep(Duration::from_micros(100));
                    } else {
                        offset += written;
                    }
                }
            }
            eprintln!("Audio output thread finished");
        });
        self.audio_output_thread = Some(output_handle);

        // Set up CPAL output
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("No audio output device");
        let config = device.default_output_config().expect("No default config");
        let sample_rate = config.sample_rate();
        let channels = config.channels();

        if sample_rate != SAMPLE_RATE || channels != CHANNELS {
            eprintln!(
                "Warning: audio format mismatch – expected {} Hz, {} channels",
                SAMPLE_RATE, CHANNELS
            );
        }

        let stream_config = config.config();
        let stream = device
            .build_output_stream(
                stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let n = consumer.pop_slice(data);
                    if n < data.len() {
                        data[n..].fill(0.0);
                    }
                },
                |err| eprintln!("Audio error: {}", err),
                None,
            )
            .unwrap();

        stream.play().unwrap();
        self.audio_stream = Some(stream);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window and renderer
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

        // Start video pipeline
        self.init_pipeline();

        // Start audio output
        self.start_audio_output();

        // Start audio test if requested
        if let Some(config) = parse_args() {
            self.start_audio_test(config);
        }

        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let renderer = self.renderer.as_mut().unwrap();
                let frame = self
                    .pipeline
                    .as_ref()
                    .unwrap()
                    .pop_processed_frame()
                    .map(|f| f.data);
                renderer.render(frame);
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
