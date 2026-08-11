use mlfb_av_core::audio::{CALLBACK_SAMPLES, CHANNELS, SAMPLE_RATE, SPSC_CAPACITY};
use mlfb_av_core::memory::{
    PackedIndex, STATE_CONSUMED, STATE_INGESTED, STATE_ML_COMMITTED, SlotPool,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam::queue::ArrayQueue;
use minstant::Instant;
use ringbuf::HeapRb;
use ringbuf::traits::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

// === FIXED SIZE ===
const AUDIO_SLOT_SIZE: usize = 8192; // 2048 f32 * 4 bytes, matches CALLBACK_SAMPLES
const N_A: usize = 64;
const MAX_CALLBACK_US: u64 = 100;

fn main() {
    // 1. Create the slot pool.
    let pool = Arc::new(SlotPool::<AUDIO_SLOT_SIZE>::new(N_A));

    // 2. Create the queues.
    let audio_ingested = Arc::new(ArrayQueue::<PackedIndex>::new(N_A));
    let audio_ml_ready = Arc::new(ArrayQueue::<PackedIndex>::new(N_A));

    // 3. Create the SPSC ring buffer for audio output.
    let rb = HeapRb::<f32>::new(SPSC_CAPACITY);
    let (mut producer, mut consumer) = rb.split();

    // 4. Shared shutdown flag.
    let running = Arc::new(AtomicBool::new(true));

    // 5. Ingest thread.
    let pool_ingest = Arc::clone(&pool);
    let audio_ingested_producer = Arc::clone(&audio_ingested);
    let running_ingest = running.clone();
    let ingest_handle = thread::spawn(move || {
        let mut count = 0u64;
        while running_ingest.load(Ordering::Acquire) {
            if let Some(packed) = pool_ingest.try_claim() {
                pool_ingest.with_payload_mut(packed, |payload| {
                    let samples = payload.len() / 4; // now 2048
                    let f32_slice = unsafe {
                        std::slice::from_raw_parts_mut(payload.as_mut_ptr() as *mut f32, samples)
                    };
                    for (i, sample) in f32_slice.iter_mut().enumerate() {
                        let phase = (count as f32 + i as f32) / (SAMPLE_RATE as f32 / 440.0);
                        *sample = (phase * 2.0 * std::f32::consts::PI).sin() * 0.3;
                    }
                    count += 1;
                });
                audio_ingested_producer.push(packed).expect("Queue full");
            } else {
                thread::sleep(Duration::from_micros(100));
            }
        }
    });

    // 6. ML worker thread.
    let pool_ml = Arc::clone(&pool);
    let audio_ingested_consumer = Arc::clone(&audio_ingested);
    let audio_ml_ready_producer = Arc::clone(&audio_ml_ready);
    let running_ml = running.clone();
    let ml_handle = thread::spawn(move || {
        while running_ml.load(Ordering::Acquire) {
            if let Some(packed) = audio_ingested_consumer.pop() {
                pool_ml.with_payload_mut(packed, |payload| {
                    let samples = payload.len() / 4;
                    let f32_slice = unsafe {
                        std::slice::from_raw_parts_mut(payload.as_mut_ptr() as *mut f32, samples)
                    };
                    for sample in f32_slice.iter_mut() {
                        *sample *= 0.5;
                    }
                });
                pool_ml
                    .transition_state(packed, STATE_INGESTED, STATE_ML_COMMITTED)
                    .expect("State transition failed");
                audio_ml_ready_producer.push(packed).expect("Queue full");
            } else {
                thread::sleep(Duration::from_micros(100));
            }
        }
    });

    // 7. Output writer thread.
    let pool_output = Arc::clone(&pool);
    let audio_ml_ready_consumer = Arc::clone(&audio_ml_ready);
    let running_output = running.clone();
    let output_handle = thread::spawn(move || {
        while running_output.load(Ordering::Acquire) {
            if let Some(packed) = audio_ml_ready_consumer.pop() {
                let slice = pool_output.with_payload_mut(packed, |payload| {
                    let samples = payload.len() / 4;
                    unsafe {
                        std::slice::from_raw_parts_mut(payload.as_mut_ptr() as *mut f32, samples)
                    }
                });
                // slice length is now 2048, exactly CALLBACK_SAMPLES
                let samples_needed = CALLBACK_SAMPLES;
                if slice.len() >= samples_needed {
                    let mut pos = 0;
                    while pos < samples_needed {
                        let n = producer.push_slice(&slice[pos..pos + samples_needed]);
                        if n < samples_needed {
                            thread::sleep(Duration::from_micros(100));
                        } else {
                            pos += n;
                        }
                    }
                } else {
                    eprintln!(
                        "Payload too small: expected {} got {}",
                        CALLBACK_SAMPLES,
                        slice.len()
                    );
                }
                pool_output
                    .transition_state(packed, STATE_ML_COMMITTED, STATE_CONSUMED)
                    .expect("State transition failed");
                pool_output.release_audio(packed);
            } else {
                thread::sleep(Duration::from_micros(100));
            }
        }
    });

    // 8. CPAL setup.
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("No audio output device");
    let config = device.default_output_config().expect("No default config");
    let sample_rate = config.sample_rate();
    let channels = config.channels();

    assert_eq!(sample_rate, SAMPLE_RATE, "Sample rate mismatch");
    assert_eq!(channels as u16, CHANNELS, "Channel count mismatch");

    let stream_config = config.config();
    let max_duration = Arc::new(AtomicU64::new(0));
    let max_duration_cb = max_duration.clone();

    let stream = device
        .build_output_stream(
            stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let start = Instant::now();

                let n = consumer.pop_slice(data);
                if n < data.len() {
                    data[n..].fill(0.0);
                }

                let elapsed_us = start.elapsed().as_micros() as u64;
                if elapsed_us > max_duration_cb.load(Ordering::Relaxed) {
                    max_duration_cb.store(elapsed_us, Ordering::Relaxed);
                }

                debug_assert!(
                    elapsed_us < MAX_CALLBACK_US,
                    "Callback took {}µs, > {}",
                    elapsed_us,
                    MAX_CALLBACK_US
                );
            },
            |err| eprintln!("Audio error: {}", err),
            None,
        )
        .unwrap();

    stream.play().unwrap();
    println!("Audio pipeline is running.");

    std::thread::sleep(std::time::Duration::from_secs(5));

    drop(stream);
    running.store(false, Ordering::Release);

    ingest_handle.join().unwrap();
    ml_handle.join().unwrap();
    output_handle.join().unwrap();

    let max_us = max_duration.load(Ordering::Relaxed);
    println!("Max callback duration: {} µs", max_us);
    if max_us < MAX_CALLBACK_US {
        println!("PASSED: audio pipeline with slot pool");
    } else {
        println!("FAILED: callback time exceeded {} µs", MAX_CALLBACK_US);
        std::process::exit(1);
    }
}
