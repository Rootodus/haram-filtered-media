use crate::sync::{AvSync, PlaybackState};
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
    pub cpal_thread: thread::JoinHandle<()>,
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

/// Very simple linear resampler for interleaved audio.
fn resample_interleaved(input: &[f32], src_rate: u32, dst_rate: u32, channels: usize) -> Vec<f32> {
    if src_rate == dst_rate || input.is_empty() {
        return input.to_vec();
    }

    let src_frames = input.len() / channels;
    if src_frames == 0 {
        return Vec::new();
    }

    let dst_frames =
        ((src_frames as f64 * dst_rate as f64 / src_rate as f64).ceil() as usize).max(1);
    let mut output = Vec::with_capacity(dst_frames * channels);

    for out_frame in 0..dst_frames {
        let src_pos = out_frame as f64 * src_rate as f64 / dst_rate as f64;
        let src_idx = src_pos.floor() as usize;
        let frac = src_pos - src_idx as f64;
        let next_idx = (src_idx + 1).min(src_frames - 1);

        for ch in 0..channels {
            let a = input[src_idx * channels + ch];
            let b = input[next_idx * channels + ch];
            let val = a as f64 * (1.0 - frac) + b as f64 * frac;
            output.push(val as f32);
        }
    }

    output
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
///
/// This function does **not** block the caller. CPAL startup and prebuffer
/// waiting happen on a background thread.
pub fn start_audio_pipeline(
    config: AudioTestConfig,
    audio_rx: Receiver<(Vec<f32>, u64)>,
    av_sync: Arc<AvSync>,
) -> Result<AudioPipelineHandles> {
    // Build the ONNX session (CPU or DML)
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

    // Query output device configuration. This is fast and non-blocking.
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No audio output device"))?;
    let default_config = device
        .default_output_config()
        .map_err(|e| anyhow!("No default output config: {e}"))?;
    let output_rate = default_config.sample_rate();
    let output_channels = default_config.channels() as usize;

    println!(
        "[AUDIO] output device: rate={}, channels={}",
        output_rate, output_channels
    );

    // Output ring buffer for CPAL
    let out_rb = HeapRb::<f32>::new(SPSC_CAPACITY);
    let (mut out_prod, mut out_cons) = out_rb.split();

    // Clone av_sync for the processing thread
    let av_sync_process = av_sync.clone();

    // Spawn processing thread
    let process_handle = thread::spawn(move || {
        let mut next_output_pts_ns: Option<u64> = None;

        let _ = thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Min);

        let mut input_buffer: Vec<f32> = Vec::new();
        let mut overlap_out: Vec<f32> = vec![0.0; window_samples * 2];

        loop {
            match audio_rx.recv() {
                Ok((chunk, pts)) => {
                    if next_output_pts_ns.is_none() {
                        next_output_pts_ns = Some(pts);
                    }
                    input_buffer.extend_from_slice(&chunk);

                    while input_buffer.len() >= window_samples * 2 {
                        let mut window_input: Vec<f32> =
                            input_buffer[..window_samples * 2].to_vec();

                        apply_window(&mut window_input, &hann);

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

                        for i in 0..(window_samples * 2) {
                            let window_idx = i / 2;
                            overlap_out[i] += processed[i] * hann[window_idx];
                        }

                        let output_hop: Vec<f32> = overlap_out[..hop_samples * 2].to_vec();

                        overlap_out.drain(0..(hop_samples * 2));
                        overlap_out.extend_from_slice(&vec![0.0; hop_samples * 2]);

                        input_buffer.drain(0..(hop_samples * 2));

                        let current_pts = next_output_pts_ns.unwrap_or(0);
                        next_output_pts_ns = Some(
                            current_pts + (hop_samples as u64 * 1_000_000_000 / SAMPLE_RATE as u64),
                        );

                        if !av_sync_process.audio_clock().is_initialized() {
                            av_sync_process.audio_clock().set_base_pts(current_pts);
                        }

                        av_sync_process.gate_audio_output(current_pts);

                        // Resample to output device rate if necessary.
                        let output_hop_resampled = if output_rate != SAMPLE_RATE {
                            resample_interleaved(
                                &output_hop,
                                SAMPLE_RATE,
                                output_rate,
                                output_channels,
                            )
                        } else {
                            output_hop
                        };

                        // Push output_hop_resampled losslessly into ring buffer.
                        let mut written = 0;
                        while written < output_hop_resampled.len() {
                            written += out_prod.push_slice(&output_hop_resampled[written..]);
                            if written < output_hop_resampled.len() {
                                thread::sleep(Duration::from_millis(1));
                            }
                        }
                    }
                }
                Err(_) => {
                    println!("Audio channel closed. Flushing...");
                    break;
                }
            }
        }

        println!("Audio processing thread finished");
    });

    // Spawn CPAL thread: waits for prebuffer, then starts playback.
    let av_sync_cpal = av_sync.clone();
    let av_sync_cpal_closure = av_sync_cpal.clone();

    let cpal_handle = thread::spawn(move || {
        // Wait for at least ~100 ms of pre‑buffer.
        let prebuffer_frames = (output_rate as usize / 10) * output_channels;
        while out_cons.occupied_len() < prebuffer_frames {
            thread::sleep(Duration::from_millis(10));
        }

        let stream = device
            .build_output_stream(
                default_config.config(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let n = out_cons.pop_slice(data);
                    if n < data.len() {
                        data[n..].fill(0.0);
                    }
                    let frames_played = n / output_channels;
                    av_sync_cpal_closure
                        .audio_clock()
                        .advance_by_frames(frames_played);
                },
                |err| eprintln!("Audio error: {err}"),
                None,
            )
            .expect("Failed to build output stream");

        stream.play().expect("Failed to start audio stream");
        println!("[AUDIO] playback started");

        av_sync_cpal.set_state(PlaybackState::Playing);

        // Keep the stream alive indefinitely.
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
    });

    Ok(AudioPipelineHandles {
        process_thread: process_handle,
        cpal_thread: cpal_handle,
    })
}
