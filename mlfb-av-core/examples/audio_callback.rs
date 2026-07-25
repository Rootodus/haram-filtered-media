use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use minstant::Instant;
use mlfb_av_core::audio::{CALLBACK_SAMPLES, CHANNELS, SAMPLE_RATE, SPSC_CAPACITY};
use ringbuf::HeapRb;
use ringbuf::traits::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

const MAX_CALLBACK_US: u64 = 100;

fn main() {
    let rb = HeapRb::<f32>::new(SPSC_CAPACITY);
    let (mut producer, mut consumer) = rb.split();

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    let zero_buffer = vec![0.0f32; CALLBACK_SAMPLES];

    let writer_handle = thread::spawn(move || {
        while running_clone.load(Ordering::Acquire) {
            if producer.push_slice(&zero_buffer) < CALLBACK_SAMPLES {
                thread::sleep(Duration::from_micros(100));
            }
        }
    });

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

    // Clone for the closure.
    let max_duration_clone = max_duration.clone();

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
                if elapsed_us > max_duration_clone.load(Ordering::Relaxed) {
                    max_duration_clone.store(elapsed_us, Ordering::Relaxed);
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
    println!("Audio stream is playing. Press Enter to stop...");

    std::thread::sleep(std::time::Duration::from_secs(5));

    drop(stream);
    running.store(false, Ordering::Release);
    writer_handle.join().unwrap();

    let max_us = max_duration.load(Ordering::Relaxed);
    println!("Max callback duration: {} µs", max_us);
    if max_us < MAX_CALLBACK_US {
        println!("PASSED: callback time under {} µs", MAX_CALLBACK_US);
    } else {
        println!("FAILED: callback time exceeded {} µs", MAX_CALLBACK_US);
        std::process::exit(1);
    }
}
