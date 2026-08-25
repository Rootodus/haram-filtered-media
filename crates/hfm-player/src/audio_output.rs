use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::Receiver;
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Producer, Split};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use hfm_core::coordination::{AudioClock, BufferingFlag, SeekGeneration};
use hfm_core::media_messages::ProcessedAudioChunk;

const SPSC_CAPACITY: usize = 1_048_576; // ~12 s of stereo float

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

/// Spawn the audio output worker thread.
///
/// This worker:
/// 1. Receives `ProcessedAudioChunk`s
/// 2. Resamples them if necessary to the output device rate
/// 3. Pushes them into a ring buffer consumed by CPAL
/// 4. Advances the shared `AudioClock` in the CPAL callback
/// 5. Updates the `BufferingFlag` based on ring buffer occupancy
/// 6. Clears the ring buffer when a new seek generation arrives
///
/// It does not know about GStreamer, ONNX, or video.
pub fn spawn_audio_output(
    rx: Receiver<ProcessedAudioChunk>,
    audio_clock: Arc<AudioClock>,
    buffering: Arc<BufferingFlag>,
    generation: Arc<SeekGeneration>,
    clear_audio: Arc<AtomicBool>,
    source_rate: u32,
    _channels: u16,
    volume: Arc<AtomicU8>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("audio-output".to_string())
        .spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_output_device() {
                Some(d) => d,
                None => {
                    eprintln!("No audio output device");
                    return;
                }
            };

            let default_config = match device.default_output_config() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("No default output config: {e}");
                    return;
                }
            };

            let output_rate = default_config.sample_rate();
            let output_channels = default_config.channels() as usize;

            println!(
                "[AUDIO_OUT] device rate={}, channels={}",
                output_rate, output_channels
            );

            let out_rb = HeapRb::<f32>::new(SPSC_CAPACITY);
            let (mut out_prod, mut out_cons) = out_rb.split();

            let low_watermark = output_rate as usize * output_channels * 3 / 2; // 1.5 s
            let high_watermark = output_rate as usize * output_channels * 5; // 5 s

            let mut base_pts_initialized = false;
            let mut current_generation = generation.current();

            // Helper to drain all samples from the consumer side.
            fn drain_consumer<C: Consumer<Item = f32>>(out_cons: &mut C) {
                while out_cons.try_pop().is_some() {}
            }

            // Prebuffer: wait until we have at least high_watermark samples before
            // starting CPAL playback.
            while out_prod.occupied_len() < high_watermark {
                match rx.recv() {
                    Ok(chunk) => {
                        if chunk.generation != generation.current() {
                            continue; // stale chunk
                        }

                        // Detect generation change and restart prebuffer.
                        if chunk.generation != current_generation {
                            drain_consumer(&mut out_cons);
                            current_generation = chunk.generation;
                            base_pts_initialized = false;
                        }

                        if !base_pts_initialized {
                            audio_clock.set_base_pts(chunk.pts_ns);
                            base_pts_initialized = true;
                        }

                        let samples = if output_rate != source_rate {
                            resample_interleaved(
                                &chunk.samples,
                                source_rate,
                                output_rate,
                                output_channels,
                            )
                        } else {
                            chunk.samples
                        };

                        let mut written = 0;
                        while written < samples.len() {
                            written += out_prod.push_slice(&samples[written..]);
                            if written < samples.len() {
                                thread::sleep(Duration::from_millis(1));
                            }
                        }
                    }
                    Err(_) => return,
                }
            }

            buffering.set(false);

            let audio_clock_cb = audio_clock.clone();
            let buffering_cb = buffering.clone();
            let clear_audio_cb = clear_audio.clone();

            let stream = device
                .build_output_stream(
                    default_config.config(),
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        // If a seek occurred, clear all old audio before playing.
                        if clear_audio_cb.load(Ordering::Acquire) {
                            data.fill(0.0);
                            while out_cons.try_pop().is_some() {}
                            clear_audio_cb.store(false, Ordering::Release);
                            return;
                        }

                        let vol = volume.load(Ordering::Relaxed) as f32 / 100.0;
                        for sample in data.iter_mut() {
                            *sample *= vol;
                        }

                        let occupied = out_cons.occupied_len();

                        // Not enough audio for this callback. Pause both streams before muting.
                        if occupied < data.len() {
                            buffering_cb.set(true);
                            data.fill(0.0);
                            return;
                        }

                        // Enough data. Update watermark state normally.
                        if occupied < low_watermark {
                            buffering_cb.set(true);
                        } else if occupied > high_watermark {
                            buffering_cb.set(false);
                        }

                        let n = out_cons.pop_slice(data);
                        if n < data.len() {
                            data[n..].fill(0.0);
                        }

                        let frames_played = n / output_channels;
                        if frames_played > 0 {
                            // Pass the actual output device rate (e.g., 48 kHz)
                            audio_clock_cb.advance_by_frames(frames_played, output_rate);
                        }
                    },
                    |err| eprintln!("Audio error: {err}"),
                    None,
                )
                .expect("Failed to build output stream");

            stream.play().expect("Failed to start audio stream");
            println!("[AUDIO_OUT] playback started");

            // Main receive loop.
            loop {
                match rx.recv() {
                    Ok(chunk) => {
                        if chunk.generation != generation.current() {
                            continue; // stale
                        }

                        // New generation: request the callback to clear old audio.
                        if chunk.generation != current_generation {
                            clear_audio.store(true, Ordering::SeqCst);

                            // Wait until callback has drained old samples.
                            while clear_audio.load(Ordering::Acquire) {
                                thread::sleep(Duration::from_millis(1));
                            }

                            current_generation = chunk.generation;
                            audio_clock.reset();
                            base_pts_initialized = false;
                        }

                        if !base_pts_initialized {
                            audio_clock.set_base_pts(chunk.pts_ns);
                            base_pts_initialized = true;
                        }

                        let samples = if output_rate != source_rate {
                            resample_interleaved(
                                &chunk.samples,
                                source_rate,
                                output_rate,
                                output_channels,
                            )
                        } else {
                            chunk.samples
                        };

                        let mut written = 0;
                        while written < samples.len() {
                            // Stop pushing if generation changes mid-push.
                            if chunk.generation != generation.current() {
                                break;
                            }

                            written += out_prod.push_slice(&samples[written..]);
                            if written < samples.len() {
                                thread::sleep(Duration::from_millis(1));
                            }
                        }
                    }
                    Err(_) => break,
                }
            }

            println!("Audio output thread finished");
        })
        .expect("Failed to spawn audio output thread")
}
