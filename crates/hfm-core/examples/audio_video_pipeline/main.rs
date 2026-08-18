mod gst_source;
mod renderer;

use anyhow::Result;
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

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Producer, Split};

// Audio constants
const SAMPLE_RATE: u32 = 44100; // Must match the model (HT-Demucs expects 44.1 kHz)
const CHANNELS: u16 = 2;
const SPSC_CAPACITY: usize = 1_048_576; // ~12 seconds of stereo float

// Audio processing constants
const WINDOW_SAMPLES: usize = 343980; // HT-Demucs fixed window
const OVERLAP_RATIO: f32 = 0.25; // 25% overlap
const STEP_SAMPLES: usize = ((WINDOW_SAMPLES as f32) * (1.0 - OVERLAP_RATIO)) as usize; // 257,985

// We'll reuse the session builder from the audio_pipeline_benchmark.
// We'll make it public in a separate module or copy it here.
// For brevity, I'll include it inline (but you can move to a shared module).

mod audio_bench {
    use anyhow::{Result, anyhow};
    use ort::session::Session;
    use ort::session::builder::GraphOptimizationLevel;

    pub fn build_session(path: &str, backend: &str) -> Result<Session> {
        match backend {
            "cpu" => {
                let mut builder = Session::builder()
                    .map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
                builder = builder
                    .with_optimization_level(GraphOptimizationLevel::Level1)
                    .map_err(|e| anyhow!("Failed to set optimization level: {}", e))?;
                builder = builder
                    .with_intra_threads(4)
                    .map_err(|e| anyhow!("Failed to set intra threads: {}", e))?;
                builder = builder
                    .with_execution_providers([ort::ep::CPU::default().build()])
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
                    let mut dml_builder = Session::builder()
                        .map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
                    dml_builder = dml_builder
                        .with_optimization_level(GraphOptimizationLevel::Level1)
                        .map_err(|e| anyhow!("Failed to set optimization level: {}", e))?;
                    dml_builder = dml_builder
                        .with_intra_threads(1)
                        .map_err(|e| anyhow!("Failed to set intra threads: {}", e))?;
                    dml_builder = dml_builder
                        .with_execution_providers([ort::ep::DirectML::default().build()])
                        .map_err(|e| anyhow!("Failed to set DirectML provider: {}", e))?;
                    match dml_builder.commit_from_file(path) {
                        Ok(session) => {
                            println!("SUCCESS: DirectML hardware backend is active.");
                            Ok(session)
                        }
                        Err(e) => {
                            println!("DirectML failed: {}. Falling back to CPU...", e);
                            let mut cpu_builder = Session::builder().map_err(|e| {
                                anyhow!("Failed to create CPU fallback builder: {}", e)
                            })?;
                            cpu_builder = cpu_builder
                                .with_optimization_level(GraphOptimizationLevel::Level1)
                                .map_err(|e| {
                                    anyhow!("Failed to set CPU optimization level: {}", e)
                                })?;
                            cpu_builder = cpu_builder
                                .with_intra_threads(4)
                                .map_err(|e| anyhow!("Failed to set CPU intra threads: {}", e))?;
                            cpu_builder = cpu_builder
                                .with_execution_providers([ort::ep::CPU::default().build()])
                                .map_err(|e| anyhow!("Failed to set CPU provider: {}", e))?;
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
}

struct AudioTestConfig {
    model_path: String,
    backend: String,
    window_size: usize,
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

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pipeline: Option<PipelineController>,
    audio_pull_thread: Option<thread::JoinHandle<()>>,
    audio_process_thread: Option<thread::JoinHandle<()>>,
    audio_output_stream: Option<cpal::Stream>,
    frame_count: u32,
    fps_timer: Instant,
    // We'll keep the raw producer and output consumer in the struct to avoid dropping them.
    _raw_prod: Option<HeapRb<f32>>, // Actually we need the split parts; easier to store as Option<Producer> and Consumer.
                                    // But we'll manage them inside the threads.
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            pipeline: None,
            audio_pull_thread: None,
            audio_process_thread: None,
            audio_output_stream: None,
            frame_count: 0,
            fps_timer: Instant::now(),
            _raw_prod: None,
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

    fn start_audio_pipeline(&mut self, config: AudioTestConfig) -> Result<()> {
        // Build ONNX session
        let mut session = audio_bench::build_session(&config.model_path, &config.backend)?;

        // Create ring buffers for raw and processed audio
        let raw_rb = HeapRb::<f32>::new(SPSC_CAPACITY);
        let (mut raw_prod, mut raw_cons) = raw_rb.split();

        let out_rb = HeapRb::<f32>::new(SPSC_CAPACITY);
        let (mut out_prod, mut out_cons) = out_rb.split();

        // Pre-compute Hann window
        let window_samples = config.window_size;
        let hann: Vec<f32> = (0..window_samples)
            .map(|i| {
                0.5 * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32 / (window_samples - 1) as f32).cos())
            })
            .collect();

        // Spawn audio pull thread
        let mut gst_source = GstSource::new().expect("Failed to create GStreamer source for audio");
        let pull_handle = thread::spawn(move || {
            while let Some((samples, _pts)) = gst_source.pull_audio_frame() {
                let mut offset = 0;
                while offset < samples.len() {
                    let written = raw_prod.push_slice(&samples[offset..]);
                    if written == 0 {
                        thread::sleep(Duration::from_micros(100));
                    } else {
                        offset += written;
                    }
                }
            }
            eprintln!("Audio pull thread finished");
        });
        self.audio_pull_thread = Some(pull_handle);

        // Spawn processing thread
        let process_handle = thread::spawn(move || {
            let mut buffer = Vec::with_capacity(window_samples * 2); // stereo
            let mut overlap_buf = vec![0.0f32; window_samples * 2 + STEP_SAMPLES * 2];
            let mut out_offset = 0;

            loop {
                // Read raw samples
                let mut chunk = vec![0.0f32; 4096]; // fixed‑size buffer
                let n = raw_cons.pop_slice(&mut chunk);
                if n == 0 {
                    thread::sleep(Duration::from_micros(100));
                    continue;
                }
                chunk.truncate(n);
                buffer.extend_from_slice(&chunk);

                // Process full windows
                while buffer.len() >= window_samples * 2 {
                    let window: Vec<f32> = buffer.drain(0..window_samples * 2).collect();
                    // Convert to tensor [1, 2, window_samples]
                    let arr = ndarray::Array::from_shape_vec((1, 2, window_samples), window)
                        .expect("Failed to create ndarray");
                    let input_value =
                        ort::value::Value::from_array(arr).expect("Failed to create value");
                    let t0 = Instant::now();
                    let outputs = session
                        .run(ort::inputs![input_value])
                        .expect("Inference failed");
                    let elapsed = t0.elapsed().as_secs_f64();
                    println!("Audio inference time: {:.3}s", elapsed);

                    let separated = &outputs[0];
                    let separated_arr = separated
                        .try_extract_array::<f32>()
                        .expect("Failed to extract tensor");
                    let vocals = separated_arr.slice(ndarray::s![0, 3, .., ..]); // -> [2, window_samples]
                    // Apply Hann and add to overlap buffer
                    for ch in 0..2 {
                        for i in 0..window_samples {
                            overlap_buf[out_offset + i * 2 + ch] += vocals[[ch, i]] * hann[i];
                        }
                    }
                    out_offset += STEP_SAMPLES * 2;

                    // If we have enough samples ready, push to output ring buffer
                    if out_offset >= STEP_SAMPLES * 2 {
                        let ready = &overlap_buf[0..STEP_SAMPLES * 2];
                        let mut written = 0;
                        while written < ready.len() {
                            let n = out_prod.push_slice(&ready[written..]);
                            if n == 0 {
                                thread::sleep(Duration::from_micros(100));
                            } else {
                                written += n;
                            }
                        }
                        // Shift overlap buffer
                        for i in 0..(overlap_buf.len() - STEP_SAMPLES * 2) {
                            overlap_buf[i] = overlap_buf[i + STEP_SAMPLES * 2];
                        }
                        for i in (overlap_buf.len() - STEP_SAMPLES * 2)..overlap_buf.len() {
                            overlap_buf[i] = 0.0;
                        }
                        out_offset -= STEP_SAMPLES * 2;
                    }
                }
            }
        });
        self.audio_process_thread = Some(process_handle);

        // Set up CPAL output (unchanged, but now reading from out_cons)
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
                    let n = out_cons.pop_slice(data);
                    if n < data.len() {
                        data[n..].fill(0.0);
                    }
                },
                |err| eprintln!("Audio error: {}", err),
                None,
            )
            .unwrap();

        stream.play().unwrap();
        self.audio_output_stream = Some(stream);

        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("Audio+Video Pipeline (with Music Removal)")
                        .with_inner_size(winit::dpi::LogicalSize::new(960, 540)),
                )
                .unwrap(),
        );
        self.window = Some(window.clone());

        let renderer = pollster::block_on(Renderer::new(window));
        self.renderer = Some(renderer);

        self.init_pipeline();

        // Start audio processing if model specified
        if let Some(config) = parse_audio_args() {
            if let Err(e) = self.start_audio_pipeline(config) {
                eprintln!("Failed to start audio pipeline: {}", e);
            }
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
