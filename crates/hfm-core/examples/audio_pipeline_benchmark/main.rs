mod gst_source;

use anyhow::{Result, anyhow};
use gst_source::GstSource;
use ndarray::Array;
use std::time::{Duration, Instant};

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
                    let mut dml_builder = Session::builder()
                        .map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
                    dml_builder = dml_builder
                        .with_parallel_execution(false)
                        .map_err(|e| anyhow!("Failed to set execution mode: {}", e))?;
                    dml_builder = dml_builder
                        .with_memory_pattern(false)
                        .map_err(|e| anyhow!("Failed to disable memory pattern: {}", e))?;
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
}

struct Args {
    model_path: String,
    backend: String,
    duration_secs: u64,
    window_size: usize,
    channels: usize,
    sample_rate: u32,
}

fn parse_args() -> Result<Args> {
    let args: Vec<String> = std::env::args().collect();
    let mut model_path = None;
    let mut backend = "cpu".to_string();
    let mut duration = 30;
    let mut window_size = 343980; // HT‑Demucs fixed window
    let mut channels = 2;
    let mut sample_rate = 44100; // HT‑Demucs expects 44.1 kHz

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--model" => {
                model_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--backend" => {
                backend = args[i + 1].clone();
                i += 2;
            }
            "--duration" => {
                duration = args[i + 1].parse().unwrap_or(30);
                i += 2;
            }
            "--window-size" => {
                window_size = args[i + 1].parse().unwrap_or(343980);
                i += 2;
            }
            "--channels" => {
                channels = args[i + 1].parse().unwrap_or(2);
                i += 2;
            }
            "--sample-rate" => {
                sample_rate = args[i + 1].parse().unwrap_or(44100);
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }
    let model_path = model_path.ok_or_else(|| anyhow!("Missing --model"))?;
    Ok(Args {
        model_path,
        backend,
        duration_secs: duration,
        window_size,
        channels,
        sample_rate,
    })
}

fn main() -> Result<()> {
    let args = parse_args()?;

    // 1. Build ONNX session
    let mut session = audio_bench::build_session(&args.model_path, &args.backend)?;

    // 2. Set up GStreamer audio source (caps already set to 44.1 kHz stereo)
    let mut source = GstSource::new()
        .map_err(|e| anyhow::anyhow!("Failed to create GStreamer source: {}", e))?;

    // 3. Accumulate samples and run inference
    let window_samples = args.window_size;
    let channels = args.channels;
    let sample_rate = args.sample_rate;

    let mut buffer = Vec::with_capacity(window_samples * channels);
    let mut inference_times = Vec::new();
    let start_time = Instant::now();
    let total_duration = Duration::from_secs(args.duration_secs);

    while start_time.elapsed() < total_duration {
        if let Some((samples, _pts)) = source.pull_audio_frame() {
            buffer.extend_from_slice(&samples);
            while buffer.len() >= window_samples * channels {
                let window: Vec<f32> = buffer.drain(0..window_samples * channels).collect();
                let arr = Array::from_shape_vec((1, channels, window_samples), window)?;
                let input_value = ort::value::Value::from_array(arr)?;
                let t0 = Instant::now();
                let _ = session.run(ort::inputs![input_value])?;
                inference_times.push(t0.elapsed().as_micros() as f64);
            }
        } else {
            break;
        }
    }

    let n = inference_times.len();
    if n == 0 {
        println!("No inferences performed (audio too short).");
        return Ok(());
    }
    let mean_us = inference_times.iter().sum::<f64>() / n as f64;
    let mut sorted = inference_times.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95_us = sorted[(n * 95 / 100).min(n - 1)];
    let window_dur = window_samples as f64 / sample_rate as f64;
    let rtf = mean_us / 1_000_000.0 / window_dur;

    println!("\n--- Audio Model Benchmark Results ---");
    println!("Backend: {}", args.backend);
    println!("Model: {}", args.model_path);
    println!(
        "Window size: {} samples ({:.2}s)",
        window_samples, window_dur
    );
    println!("Inferences: {}", n);
    println!("Mean inference: {:.1} µs", mean_us);
    println!("p95 inference: {:.1} µs", p95_us);
    println!("RTF: {:.3}", rtf);
    println!(
        "Throughput: {:.1} inf/s",
        n as f64 / args.duration_secs as f64
    );
    if rtf < 0.1 {
        println!("✅ RTF < 0.1 – suitable for real-time.");
    } else {
        println!("❌ RTF >= 0.1 – may be too slow.");
    }

    Ok(())
}
