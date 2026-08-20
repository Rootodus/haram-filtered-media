//! Pure audio processing worker.
//!
//! This module transforms raw audio chunks into processed speech PCM using
//! the HT-Demucs ONNX model.
//!
//! It does not know about:
//! - GStreamer
//! - CPAL playback
//! - wgpu rendering
//! - buffering state
//! - video
//!
//! It only:
//! 1. Receives `RawAudioChunk`
//! 2. Runs the model
//! 3. Emits `ProcessedAudioChunk` with the same PTS and generation

use crate::sync::SeekGeneration;
use crate::types::{ProcessedAudioChunk, RawAudioChunk};
use anyhow::{Result, anyhow};
use crossbeam_channel::{Receiver, Sender};
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Value;
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;

const SAMPLE_RATE: u32 = 44100;
const OVERLAP_RATIO: f32 = 0.25;

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
            let intra_threads = (cpus / 2).max(1);

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
        planar[i] = samples[i * 2];
        planar[n_samples + i] = samples[i * 2 + 1];
    }
    planar
}

/// Apply a window (multiply) to each channel of interleaved stereo data.
fn apply_window(samples: &mut [f32], window: &[f32]) {
    let n_samples = samples.len() / 2;
    for i in 0..n_samples {
        samples[i * 2] *= window[i];
        samples[i * 2 + 1] *= window[i];
    }
}

/// Run the HT-Demucs model on one stereo window and extract vocals.
fn run_inference(
    session: &mut Session,
    input_name: &str,
    output_name: &str,
    windowed_interleaved: &[f32],
    window_samples: usize,
) -> Result<Vec<f32>> {
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

    // Output shape: [1, 4, 2, window_samples]
    let n = window_samples;
    let source_offset = 3 * 2 * n; // vocals
    if raw_slice.len() < source_offset + 2 * n {
        return Err(anyhow!(
            "Output tensor too small for source index 3: got {}",
            raw_slice.len()
        ));
    }

    let left = &raw_slice[source_offset..source_offset + n];
    let right = &raw_slice[source_offset + n..source_offset + 2 * n];

    let mut interleaved = vec![0.0f32; n * 2];
    for i in 0..n {
        interleaved[i * 2] = left[i];
        interleaved[i * 2 + 1] = right[i];
    }

    Ok(interleaved)
}

/// Spawn the audio processor worker thread.
///
/// This worker is a pure transform:
/// `RawAudioChunk` -> `ProcessedAudioChunk`
///
/// It does not know about GStreamer, CPAL, wgpu, or playback state.
pub fn spawn_audio_processor(
    config: AudioTestConfig,
    rx: Receiver<RawAudioChunk>,
    tx: Sender<ProcessedAudioChunk>,
    generation: Arc<SeekGeneration>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut session = match build_session(&config.model_path, &config.backend) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Audio session build failed: {e}");
                return;
            }
        };

        let input_name = session.inputs()[0].name().to_string();
        let output_name = session.outputs()[0].name().to_string();

        let window_samples = config.window_size;
        let hop_samples = ((window_samples as f32) * (1.0 - OVERLAP_RATIO)) as usize;

        let hann: Vec<f32> = (0..window_samples)
            .map(|i| {
                0.5 * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32 / (window_samples - 1) as f32).cos())
            })
            .collect();

        let mut input_buffer: Vec<f32> = Vec::new();
        let mut overlap_out: Vec<f32> = vec![0.0; window_samples * 2];
        let mut next_output_pts_ns: Option<u64> = None;

        loop {
            match rx.recv() {
                Ok(chunk) => {
                    // If this chunk is stale, clear all internal state and wait
                    // for data from the new generation.
                    if chunk.generation != generation.current() {
                        input_buffer.clear();
                        overlap_out.fill(0.0);
                        next_output_pts_ns = None;
                        continue;
                    }

                    if next_output_pts_ns.is_none() {
                        next_output_pts_ns = Some(chunk.pts_ns);
                    }

                    input_buffer.extend_from_slice(&chunk.samples);

                    while input_buffer.len() >= window_samples * 2 {
                        let mut windowed = input_buffer[..window_samples * 2].to_vec();
                        apply_window(&mut windowed, &hann);

                        let processed = match run_inference(
                            &mut session,
                            &input_name,
                            &output_name,
                            &windowed,
                            window_samples,
                        ) {
                            Ok(p) => p,
                            Err(e) => {
                                eprintln!("Audio inference error: {e}");
                                return;
                            }
                        };

                        for i in 0..(window_samples * 2) {
                            let idx = i / 2;
                            overlap_out[i] += processed[i] * hann[idx];
                        }

                        let output_hop = overlap_out[..hop_samples * 2].to_vec();

                        overlap_out.drain(0..(hop_samples * 2));
                        overlap_out.extend_from_slice(&vec![0.0; hop_samples * 2]);
                        input_buffer.drain(0..(hop_samples * 2));

                        let current_pts = next_output_pts_ns.unwrap_or(0);
                        next_output_pts_ns = Some(
                            current_pts + (hop_samples as u64 * 1_000_000_000 / SAMPLE_RATE as u64),
                        );

                        let msg = ProcessedAudioChunk {
                            samples: output_hop,
                            pts_ns: current_pts,
                            generation: chunk.generation,
                        };

                        if tx.send(msg).is_err() {
                            return;
                        }
                    }
                }
                Err(_) => break,
            }
        }

        println!("Audio processor thread finished");
    })
}
