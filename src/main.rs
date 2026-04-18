use ml_filtered_browser::content_buffer::{ContentBuffer, Status};
use ml_filtered_browser::fetcher::fetch_stage;
use ml_filtered_browser::ml_processor::process_stage;
use ml_filtered_browser::renderer::render_stage;

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread;
use std::time::Instant;

/// --- LOG BUFFER ---

struct LogEntry {
    config: &'static str,
    input_id: String,
    iteration: usize,
    latency_ns: u128,
    status: &'static str,
}

/// --- SUMMARY ---

fn summarize(latencies: &[u128], label: &str) {
    if latencies.is_empty() {
        println!("{}: no data", label);
        return;
    }

    let sum: u128 = latencies.iter().sum();
    let mean = sum / latencies.len() as u128;
    let min = latencies.iter().min().unwrap();
    let max = latencies.iter().max().unwrap();

    println!(
        "{} -> count: {}, mean: {} ns, min: {} ns, max: {} ns",
        label,
        latencies.len(),
        mean,
        min,
        max
    );
}

/// --- WRITE LOG ONCE ---

fn flush_logs(filename: &str, logs: &[LogEntry]) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(filename)
        .expect("Cannot open log file");

    for entry in logs {
        let line = format!(
            "{},{},{},{},{}\n",
            entry.config, entry.input_id, entry.iteration, entry.latency_ns, entry.status
        );

        file.write_all(line.as_bytes()).expect("Log write failed");
    }
}

/// --- CONFIG A (PIPELINE) ---

fn run_config_a(dataset: Vec<ContentBuffer>, repetitions: usize) {
    let capacity = 100;

    let (tx1, rx1): (SyncSender<ContentBuffer>, Receiver<ContentBuffer>) = sync_channel(capacity);
    let (tx2, rx2): (
        SyncSender<(ContentBuffer, Instant)>,
        Receiver<(ContentBuffer, Instant)>,
    ) = sync_channel(capacity);

    let latencies = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    let lat_clone = latencies.clone();
    let log_clone = logs.clone();

    let render_thread = thread::spawn(move || {
        while let Ok((unit, start)) = rx2.recv() {
            let output = render_stage(unit);

            let elapsed = start.elapsed().as_nanos() as u128;

            lat_clone.lock().unwrap().push(elapsed);

            log_clone.lock().unwrap().push(LogEntry {
                config: "A",
                input_id: output.input_id,
                iteration: output.iteration,
                latency_ns: elapsed,
                status: output.status.as_str(),
            });
        }
    });

    thread::spawn(move || {
        while let Ok((unit, start)) = rx1.recv().map(|u| (u, Instant::now())) {
            let output = process_stage(unit);
            tx2.send((output, start))
                .expect("Process -> Render send failed");
        }
    });

    thread::spawn(move || {
        for base_item in dataset {
            for i in 0..repetitions {
                let mut unit = base_item.clone();
                unit.iteration = i;

                let output = fetch_stage(unit);
                tx1.send(output).expect("Fetch -> Process send failed");
            }
        }
    });

    render_thread.join().unwrap();

    let lat = latencies.lock().unwrap();
    summarize(&lat, "Config A");

    let logs = logs.lock().unwrap();
    flush_logs("EXP-ARCH-BASE-RUN-A.log", &logs);
}

/// --- CONFIG B (SYNC) ---

fn run_config_b(dataset: Vec<ContentBuffer>, repetitions: usize) {
    let mut latencies = Vec::new();
    let mut logs = Vec::new();

    for base_item in dataset {
        for i in 0..repetitions {
            let mut unit = base_item.clone();
            unit.iteration = i;

            let start = Instant::now();

            let f = fetch_stage(unit);
            let p = process_stage(f);
            let r = render_stage(p);

            let elapsed = start.elapsed().as_nanos() as u128;

            latencies.push(elapsed);

            logs.push(LogEntry {
                config: "B",
                input_id: r.input_id,
                iteration: r.iteration,
                latency_ns: elapsed,
                status: r.status.as_str(),
            });
        }
    }

    summarize(&latencies, "Config B");
    flush_logs("EXP-ARCH-BASE-RUN-B.log", &logs);
}

/// --- ENTRY ---

fn main() {
    let args: Vec<String> = env::args().collect();

    let mode_arg = args
        .iter()
        .position(|r| r == "--mode")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str());

    let mut dataset = Vec::new();
    let ids = vec!["alpha", "beta", "gamma", "delta", "epsilon"];

    for id in ids {
        dataset.push(ContentBuffer {
            input_id: id.to_string(),
            iteration: 0,
            // increased payload to make measurement meaningful
            payload: vec![0u8; 1024 * 64], // 64 KB
            status: Status::SUCCESS,
            start_time_ms: 0,
            end_time_ms: 0,
        });
    }

    let repetitions = 1000;

    let total_start = Instant::now();

    match mode_arg {
        Some("A") => run_config_a(dataset, repetitions),
        Some("B") => run_config_b(dataset, repetitions),
        _ => {
            eprintln!("Usage: benchmark --mode [A|B]");
            std::process::exit(1);
        }
    }

    println!("Total runtime: {} ms", total_start.elapsed().as_millis());
}
