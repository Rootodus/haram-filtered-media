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

const CHANNEL_CAP: usize = 128;

struct Timing {
    enqueue_time: Instant,
    process_start: Option<Instant>,
    process_end: Option<Instant>,
}

struct LogEntry {
    config: &'static str,
    input_id: String,
    iteration: usize,
    e2e_latency_ns: u128,
    queue_delay_ns: u128,
    process_latency_ns: u128,
    status: &'static str,
}

enum ExecutionMode {
    Pipeline,
    Sync,
}

fn select_mode(buf: &ContentBuffer) -> ExecutionMode {
    if buf.payload.len() > 32 * 1024 {
        ExecutionMode::Pipeline
    } else {
        ExecutionMode::Sync
    }
}

fn warmup(dataset: &[ContentBuffer]) {
    for item in dataset {
        let f = fetch_stage(item.clone());
        let p = process_stage(f);
        let _ = render_stage(p);
    }
}

fn percentile(latencies: &mut [u128], p: f64) -> u128 {
    latencies.sort_unstable();
    let idx = ((latencies.len() as f64 - 1.0) * p).round() as usize;
    latencies[idx]
}

fn summarize(latencies: &[u128], label: &str) {
    if latencies.is_empty() {
        println!("{}: no data", label);
        return;
    }

    let sum: u128 = latencies.iter().sum();
    let mean = sum / latencies.len() as u128;
    let min = latencies.iter().min().unwrap();
    let max = latencies.iter().max().unwrap();

    let mut sorted = latencies.to_vec();
    let p95 = percentile(&mut sorted, 0.95);

    println!(
        "{} -> count: {}, mean: {} ns, min: {} ns, p95: {} ns, max: {} ns",
        label,
        latencies.len(),
        mean,
        min,
        p95,
        max
    );
}

fn flush_logs(filename: &str, logs: &[LogEntry]) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(filename)
        .expect("Cannot open log file");

    for entry in logs {
        let line = format!(
            "{},{},{},{},{},{},{}\n",
            entry.config,
            entry.input_id,
            entry.iteration,
            entry.e2e_latency_ns,
            entry.queue_delay_ns,
            entry.process_latency_ns,
            entry.status
        );
        file.write_all(line.as_bytes()).unwrap();
    }
}

fn run_adaptive(dataset: Vec<ContentBuffer>, repetitions: usize) {
    let (tx_pipeline, rx_pipeline): (
        SyncSender<(ContentBuffer, Timing)>,
        Receiver<(ContentBuffer, Timing)>,
    ) = sync_channel(CHANNEL_CAP);

    let (tx_render, rx_render): (
        SyncSender<(ContentBuffer, Timing)>,
        Receiver<(ContentBuffer, Timing)>,
    ) = sync_channel(CHANNEL_CAP);

    let tx_render_pipeline = tx_render.clone();

    // --- PIPELINE WORKER ---
    let pipeline_thread = thread::spawn(move || {
        while let Ok((buf, mut timing)) = rx_pipeline.recv() {
            timing.process_start = Some(Instant::now());

            let processed = process_stage(buf);

            timing.process_end = Some(Instant::now());

            tx_render_pipeline.send((processed, timing)).unwrap();
        }
    });

    // --- RENDER + COLLECT ---
    let render_thread = thread::spawn(move || {
        let mut e2e_lat = Vec::new();
        let mut queue_lat = Vec::new();
        let mut proc_lat = Vec::new();
        let mut logs = Vec::new();

        while let Ok((unit, timing)) = rx_render.recv() {
            let end = Instant::now();

            let output = render_stage(unit);

            let enqueue = timing.enqueue_time;
            let p_start = timing.process_start.unwrap();
            let p_end = timing.process_end.unwrap();

            let e2e = end.duration_since(enqueue).as_nanos();
            let queue = p_start.duration_since(enqueue).as_nanos();
            let proc = p_end.duration_since(p_start).as_nanos();

            e2e_lat.push(e2e);
            queue_lat.push(queue);
            proc_lat.push(proc);

            logs.push(LogEntry {
                config: "PIPE",
                input_id: output.input_id,
                iteration: output.iteration,
                e2e_latency_ns: e2e,
                queue_delay_ns: queue,
                process_latency_ns: proc,
                status: output.status.as_str(),
            });
        }

        (e2e_lat, queue_lat, proc_lat, logs)
    });

    // --- SYNC ---
    let mut sync_e2e = Vec::new();
    let mut sync_proc = Vec::new();
    let mut sync_logs = Vec::new();

    let total_start = Instant::now();
    let mut total_count = 0;

    for base_item in dataset {
        for i in 0..repetitions {
            let mut unit = base_item.clone();
            unit.iteration = i;

            match select_mode(&unit) {
                ExecutionMode::Sync => {
                    let start = Instant::now();

                    let f = fetch_stage(unit);
                    let p_start = Instant::now();
                    let p = process_stage(f);
                    let p_end = Instant::now();
                    let r = render_stage(p);

                    let end = Instant::now();

                    let e2e = end.duration_since(start).as_nanos();
                    let proc = p_end.duration_since(p_start).as_nanos();

                    sync_e2e.push(e2e);
                    sync_proc.push(proc);

                    sync_logs.push(LogEntry {
                        config: "SYNC",
                        input_id: r.input_id,
                        iteration: r.iteration,
                        e2e_latency_ns: e2e,
                        queue_delay_ns: 0,
                        process_latency_ns: proc,
                        status: r.status.as_str(),
                    });

                    total_count += 1;
                }
                ExecutionMode::Pipeline => {
                    let f = fetch_stage(unit);

                    let timing = Timing {
                        enqueue_time: Instant::now(),
                        process_start: None,
                        process_end: None,
                    };

                    tx_pipeline.send((f, timing)).unwrap();
                    total_count += 1;
                }
            }
        }
    }

    drop(tx_pipeline);
    pipeline_thread.join().unwrap();
    drop(tx_render);

    let (pipe_e2e, pipe_queue, pipe_proc, pipe_logs) = render_thread.join().unwrap();

    let total_time = total_start.elapsed().as_secs_f64();
    let throughput = total_count as f64 / total_time;

    // --- OUTPUT ---
    summarize(&sync_e2e, "SYNC E2E");
    summarize(&sync_proc, "SYNC PROCESS");

    summarize(&pipe_e2e, "PIPE E2E");
    summarize(&pipe_queue, "PIPE QUEUE");
    summarize(&pipe_proc, "PIPE PROCESS");

    println!("Throughput: {:.2} items/sec", throughput);

    flush_logs("RUN-SYNC.log", &sync_logs);
    flush_logs("RUN-PIPE.log", &pipe_logs);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut dataset = Vec::new();
    let ids = vec!["alpha", "beta", "gamma", "delta", "epsilon"];

    for (i, id) in ids.iter().enumerate() {
        let size = match i % 3 {
            0 => 1024,
            1 => 8 * 1024,
            _ => 128 * 1024,
        };

        dataset.push(ContentBuffer {
            input_id: id.to_string(),
            iteration: 0,
            payload: vec![0u8; size],
            status: Status::SUCCESS,
            start_time_ms: 0,
            end_time_ms: 0,
        });
    }

    let repetitions = 1000;

    warmup(&dataset);

    match args.get(1).map(|s| s.as_str()) {
        Some("adaptive") => run_adaptive(dataset, repetitions),
        _ => {
            eprintln!("Usage: cargo run --release -- adaptive");
            std::process::exit(1);
        }
    }
}
