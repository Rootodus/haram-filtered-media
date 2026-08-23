//! HT‑Demucs audio processing worker.
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

// ============================================================================
// ASSET ACKNOWLEDGMENT & ATTRIBUTION:
// This module executes inference against an ONNX export of the HT‑Demucs
// (Hybrid Transformer Demucs) music source separation model.
//
// Source Project: facebookresearch/demucs
// Model Architecture: Hybrid Transformer Demucs (HT‑Demucs)
// Research Paper: "Hybrid Transformers for Music Source Separation"
//                  https://arxiv.org/abs/2211.08553
// Original Authors: Simon Rouard, Francisco Massa, Alexandre Défossez (Meta AI)
// Source Repository: https://github.com/facebookresearch/demucs
//
// ONNX Export: StemSplitio/htdemucs-ft-vocals-onnx
// Repository: https://huggingface.co/StemSplitio/htdemucs-ft-vocals-onnx
//
// License: MIT
// ============================================================================

use crate::coordination::SeekGeneration;
use crate::media_messages::{ProcessedAudioChunk, RawAudioChunk};
use crate::ml::{SessionConfig, build_session};
use anyhow::{Result, anyhow};
use crossbeam_channel::{Receiver, Sender};
use ort::session::Session;
use ort::value::Value;
use std::sync::Arc;
use std::thread::JoinHandle;

const SAMPLE_RATE: u32 = 44100;
const OVERLAP_RATIO: f32 = 0.25;

/// Configuration for the HT‑Demucs audio processor.
#[derive(Debug, Clone)]
pub struct DemucsConfig {
    pub model_path: String,
    pub backend: String, // "cpu" or "dml" – will be converted to SessionConfig
    pub window_size: usize,
}

impl DemucsConfig {
    /// Convert the string backend to a `SessionConfig`.
    pub fn to_session_config(&self) -> SessionConfig {
        match self.backend.as_str() {
            "dml" => {
                // Use DirectML for audio, but keep CPU fallback enabled.
                let mut cfg = SessionConfig::video_default(); // reuse video defaults for DML
                cfg.provider = crate::ml::ExecutionProvider::DirectML;
                cfg.intra_threads = 1;
                cfg.disable_cpu_fallback = false;
                cfg
            }
            _ => {
                // CPU backend
                SessionConfig::audio_default()
            }
        }
    }
}

/// Spawn the HT‑Demucs worker thread.
///
/// This worker is a pure transform:
/// `RawAudioChunk` -> `ProcessedAudioChunk`
///
/// It does not know about GStreamer, CPAL, wgpu, or playback state.
pub fn spawn_demucs_worker(
    config: DemucsConfig,
    rx: Receiver<RawAudioChunk>,
    tx: Sender<ProcessedAudioChunk>,
    generation: Arc<SeekGeneration>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("demucs-worker".to_string())
        .spawn(move || {
            let session_config = config.to_session_config();
            let mut session = match build_session(&config.model_path, session_config) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Demucs session build failed: {e}");
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
                        - (2.0 * std::f32::consts::PI * i as f32 / (window_samples - 1) as f32)
                            .cos())
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
                                    eprintln!("Demucs inference error: {e}");
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
                                current_pts
                                    + (hop_samples as u64 * 1_000_000_000 / SAMPLE_RATE as u64),
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

            println!("Demucs worker thread finished");
        })
        .expect("Failed to spawn Demucs worker thread")
}

// Helper functions
fn apply_window(samples: &mut [f32], window: &[f32]) {
    let n_samples = samples.len() / 2;
    for i in 0..n_samples {
        samples[i * 2] *= window[i];
        samples[i * 2 + 1] *= window[i];
    }
}

fn interleaved_to_planar(samples: &[f32], n_samples: usize) -> Vec<f32> {
    let mut planar = vec![0.0f32; 2 * n_samples];
    for i in 0..n_samples {
        planar[i] = samples[i * 2];
        planar[n_samples + i] = samples[i * 2 + 1];
    }
    planar
}

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
