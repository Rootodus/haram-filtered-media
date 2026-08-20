use crate::sync::AvSync;
use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Receiver;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Value;
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// Constants shared with main.rs
const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 2;
const SPSC_CAPACITY: usize = 1_048_576; // ~12 s of stereo float
const WINDOW_SAMPLES: usize = 343_980; // HT-Demucs fixed window
const OVERLAP_RATIO: f32 = 0.25;
const STEP_SAMPLES: usize = ((WINDOW_SAMPLES as f32) * (1.0 - OVERLAP_RATIO)) as usize; // 257,985

/// Handles returned by the audio processor so the app can keep them alive.
pub struct AudioPipelineHandles {
    pub process_thread: thread::JoinHandle<()>,
    pub output_stream: cpal::Stream,
}

#[derive(Debug, Clone)]
pub struct AudioTestConfig {
    pub model_path: String,
    pub backend: String,
    pub window_size: usize,
}

fn build_session(path: &str, backend: &str) -> Result<Session> {
    match backend {
        "cpu" => {
            let cpus = thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            let intra_threads = cpus.saturating_sub(1).max(1);

            let mut builder =
                Session::builder().map_err(|e| anyhow!("Failed to create session builder: {e}"))?;
            builder = builder
                .with_optimization_level(GraphOptimizationLevel::Level1)
                .map_err(|e| anyhow!("Failed to set optimization level: {e}"))?;
            builder = builder
                .with_intra_threads(intra_threads)
                .map_err(|e| anyhow!("Failed to set intra threads: {e}"))?;
            builder = builder
                .with_execution_providers([ort::ep::CPU::default().build()])
                .map_err(|e| anyhow!("Failed to set CPU provider: {e}"))?;

            let session = builder
                .commit_from_file(path)
                .map_err(|e| anyhow!("Failed to load model on CPU: {e}"))?;
            println!("SUCCESS: CPU backend active with {intra_threads} threads.");
            Ok(session)
        }
        "dml" => {
            #[cfg(target_os = "windows")]
            {
                let mut dml_builder = Session::builder()
                    .map_err(|e| anyhow!("Failed to create session builder: {e}"))?;
                dml_builder = dml_builder
                    .with_optimization_level(GraphOptimizationLevel::Level1)
                    .map_err(|e| anyhow!("Failed to set optimization level: {e}"))?;
                dml_builder = dml_builder
                    .with_intra_threads(1)
                    .map_err(|e| anyhow!("Failed to set intra threads: {e}"))?;
                dml_builder = dml_builder
                    .with_execution_providers([ort::ep::DirectML::default().build()])
                    .map_err(|e| anyhow!("Failed to set DirectML provider: {e}"))?;

                match dml_builder.commit_from_file(path) {
                    Ok(session) => {
                        println!("SUCCESS: DirectML hardware backend is active.");
                        Ok(session)
                    }
                    Err(e) => {
                        println!("DirectML failed: {e}. Falling back to CPU...");
                        build_session(path, "cpu")
                    }
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(anyhow!("DirectML is only supported on Windows"))
            }
        }
        other => Err(anyhow!("Unsupported backend: {other}")),
    }
}

/// Convert interleaved stereo samples to a planar tensor of shape [1, 2, n_samples].
fn interleaved_to_planar(samples: &[f32], n_samples: usize) -> Vec<f32> {
    let mut planar = vec![0.0f32; 2 * n_samples];
    for i in 0..n_samples {
        planar[i] = samples[i * 2]; // left
        planar[n_samples + i] = samples[i * 2 + 1]; // right
    }
    planar
}

/// Convert planar stereo tensor [1, 2, n_samples] back to interleaved Vec<f32>.
fn planar_to_interleaved(planar: &[f32], n_samples: usize) -> Vec<f32> {
    let mut interleaved = vec![0.0f32; n_samples * 2];
    for i in 0..n_samples {
        interleaved[i * 2] = planar[i];
        interleaved[i * 2 + 1] = planar[n_samples + i];
    }
    interleaved
}

/// Apply a window (multiply) to each channel of interleaved stereo data.
fn apply_window(samples: &mut [f32], window: &[f32]) {
    let n_samples = samples.len() / 2;
    for i in 0..n_samples {
        samples[i * 2] *= window[i];
        samples[i * 2 + 1] *= window[i];
    }
}

/// Run the HT-Demucs model on one stereo window.
fn run_inference(
    session: &mut Session,
    input_name: &str,
    output_name: &str,
    windowed_interleaved: &[f32],
    window_samples: usize,
) -> Result<Vec<f32>> {
    // De‑interleave to planar
    let planar = interleaved_to_planar(windowed_interleaved, window_samples);
    let input_tensor = Value::from_array(([1, 2, window_samples], planar))
        .map_err(|e| anyhow!("Failed to create input tensor: {e}"))?;

    let outputs = session
        .run(ort::inputs![input_name => input_tensor])
        .map_err(|e| anyhow!("Inference failed: {e}"))?;

    let output_tensor = outputs
        .get(output_name)
        .ok_or_else(|| anyhow!("Output tensor missing"))?;
    let (_, raw_slice) = output_tensor
        .try_extract_tensor::<f32>()
        .map_err(|e| anyhow!("Failed to extract output tensor: {e}"))?;

    // raw_slice is planar [1,2,window_samples], but we ignore batch dimension.
    Ok(planar_to_interleaved(raw_slice, window_samples))
}

/// Start the audio processing pipeline.
///
/// Consumes raw stereo samples from `audio_rx`, performs overlap‑add
/// inference with HT‑Demucs, and sends processed PCM to a CPAL output
/// stream. Returns handles for the processing thread and output stream.
pub fn start_audio_pipeline(
    config: AudioTestConfig,
    audio_rx: Receiver<(Vec<f32>, u64)>,
    av_sync: Arc<AvSync>,
) -> Result<AudioPipelineHandles> {
    // Build the ONNX session
    let mut session = build_session(&config.model_path, &config.backend)?;

    let input_name = session.inputs()[0].name().to_string();
    let output_name = session.outputs()[0].name().to_string();

    let window_samples = config.window_size;
    let hop_samples = ((window_samples as f32) * (1.0 - OVERLAP_RATIO)) as usize;

    // Pre‑compute Hann window
    let hann: Vec<f32> = (0..window_samples)
        .map(|i| {
            0.5 * (1.0
                - (2.0 * std::f32::consts::PI * i as f32 / (window_samples - 1) as f32).cos())
        })
        .collect();

    // Output ring buffer for CPAL
    let out_rb = HeapRb::<f32>::new(SPSC_CAPACITY);
    let (mut out_prod, mut out_cons) = out_rb.split();

    // Clone av_sync for the processing thread
    let av_sync_process = av_sync.clone();

    // Spawn processing thread
    let process_handle = thread::spawn(move || {
        let mut next_output_pts_ns: Option<u64> = None; // will be set when first chunk known

        let _ = thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Min);

        let mut input_buffer: Vec<f32> = Vec::new();
        let mut overlap_out: Vec<f32> = vec![0.0; window_samples * 2];

        loop {
            match audio_rx.recv() {
                Ok((chunk, pts)) => {
                    // Initialize output PTS base if not set
                    if next_output_pts_ns.is_none() {
                        next_output_pts_ns = Some(pts);
                    }

                    input_buffer.extend_from_slice(&chunk);

                    if input_buffer.len() < 100_000 {
                        println!(
                            "[AUDIO_PROC] received chunk, buffer len={}",
                            input_buffer.len()
                        );
                    }

                    while input_buffer.len() >= window_samples * 2 {
                        // Extract window
                        let mut window_input: Vec<f32> =
                            input_buffer[..window_samples * 2].to_vec();

                        // Apply input windowing
                        apply_window(&mut window_input, &hann);

                        // Run inference
                        let processed = match run_inference(
                            &mut session,
                            &input_name,
                            &output_name,
                            &window_input,
                            window_samples,
                        ) {
                            Ok(p) => p,
                            Err(e) => {
                                eprintln!("Audio inference error: {e}");
                                break;
                            }
                        };

                        // Apply output window and overlap‑add
                        for i in 0..(window_samples * 2) {
                            let window_idx = i / 2; // each sample pair shares the same window coefficient
                            overlap_out[i] += processed[i] * hann[window_idx];
                        }

                        // Emit one hop of output
                        let output_hop: Vec<f32> = overlap_out[..hop_samples * 2].to_vec();

                        // Shift overlap buffer left by hop_samples*2, zeroing the tail
                        overlap_out.drain(0..(hop_samples * 2));
                        overlap_out.extend_from_slice(&vec![0.0; hop_samples * 2]);

                        // Remove hop_samples*2 samples from input buffer
                        input_buffer.drain(0..(hop_samples * 2));

                        let current_pts = next_output_pts_ns.unwrap_or(0);
                        next_output_pts_ns = Some(
                            current_pts + (hop_samples as u64 * 1_000_000_000 / SAMPLE_RATE as u64),
                        );

                        if !av_sync_process.audio_clock().is_initialized() {
                            av_sync_process.audio_clock().set_base_pts(current_pts);
                        }

                        // Before pushing output_hop to ring buffer, gate on video progress:
                        av_sync_process.gate_audio_output(current_pts);

                        // Push output_hop losslessly into ring buffer:
                        let mut written = 0;
                        while written < output_hop.len() {
                            written += out_prod.push_slice(&output_hop[written..]);
                            println!("[AUDIO_PROC] first output hop pushed, pts={}", current_pts);
                            if written < output_hop.len() {
                                thread::sleep(Duration::from_millis(1));
                            }
                        }
                    }
                }
                Err(_) => {
                    println!("Audio channel closed. Flushing...");
                    // Flush remaining partial data if any
                    if !input_buffer.is_empty() {
                        // Optionally process a final zero‑padded window here.
                        // For now, ignore partial window.
                    }
                    // Signal EOS to output by dropping producer
                    break;
                }
            }
        }

        println!("Audio processing thread finished");
    });

    // ---- CPAL setup ---------------------------------------------------

    // Try to select a config matching our sample rate and channel count.
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No audio output device"))?;

    let supported_config = device
        .default_output_config()
        .map_err(|e| anyhow!("No default output config: {e}"))?;

    if supported_config.sample_rate() != SAMPLE_RATE || supported_config.channels() != CHANNELS {
        eprintln!(
            "Warning: audio format mismatch – expected {} Hz, {} channels",
            SAMPLE_RATE, CHANNELS
        );
    }

    let stream_config = supported_config.config();

    // Wait for at least ~100 ms of pre‑buffer before starting playback.
    println!(
        "[AUDIO] prebuffer wait begin, occupied={}",
        out_cons.occupied_len()
    );
    while out_cons.occupied_len() < (SAMPLE_RATE as usize / 10) * CHANNELS as usize {
        thread::sleep(Duration::from_millis(10));
    }
    println!("[AUDIO] prebuffer wait done");

    // Now clone av_sync for the CPAL callback and move out_cons.
    let av_sync_cpal = av_sync.clone();

    let stream = device
        .build_output_stream(
            stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let n = out_cons.pop_slice(data);
                if n < data.len() {
                    data[n..].fill(0.0);
                }
                // Advance audio clock by number of frames played.
                let frames_played = n / CHANNELS as usize;
                av_sync_cpal.audio_clock().advance_by_frames(frames_played);
            },
            |err| eprintln!("Audio error: {err}"),
            None,
        )
        .map_err(|e| anyhow!("Failed to build output stream: {e}"))?;

    stream
        .play()
        .map_err(|e| anyhow!("Failed to start audio stream: {e}"))?;
    println!("[AUDIO] stream.play() returned successfully");

    Ok(AudioPipelineHandles {
        process_thread: process_handle,
        output_stream: stream,
    })
}
