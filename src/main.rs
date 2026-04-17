// File: main.rs
// Implementation of CONTR-EXEC-BASE for Architecture Baseline Experiment (EXP-ARCH-BASE)

use ml_filtered_browser::content_buffer::{ContentBuffer, Status};
use ml_filtered_browser::fetcher::fetch_stage;
use ml_filtered_browser::ml_processor::process_stage;
use ml_filtered_browser::renderer::render_stage;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

/// --- TIMING & LOGGING UTILITIES ---

fn get_now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System clock error")
        .as_millis()
}

fn log_unit(file: &mut File, config_id: &str, buf: &ContentBuffer) {
    let latency = buf.end_time_ms.saturating_sub(buf.start_time_ms);
    // CSV Schema: config_id,input_id,iteration,start_time_ms,end_time_ms,latency_ms,status
    let line = format!(
        "{},{},{},{},{},{},{}\n",
        config_id,
        buf.input_id,
        buf.iteration,
        buf.start_time_ms,
        buf.end_time_ms,
        latency,
        buf.status.as_str()
    );
    file.write_all(line.as_bytes()).expect("Log write failed");
    file.flush().expect("Log flush failed"); // Ensure append-only, unbuffered logging
}

/// --- CONFIGURATION A: STAGED PIPELINE ---

fn run_config_a(dataset: Vec<ContentBuffer>, repetitions: usize) {
    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("EXP-ARCH-BASE-RUN-A.log")
        .expect("Cannot open log A");

    // Channels MUST have fixed capacity defined at startup
    let capacity = 100;
    // Exactly 2 bounded channels: Fetch -> Process, Process -> Render
    let (tx1, rx1): (SyncSender<ContentBuffer>, Receiver<ContentBuffer>) = sync_channel(capacity);
    let (tx2, rx2): (SyncSender<ContentBuffer>, Receiver<ContentBuffer>) = sync_channel(capacity);

    // RenderStage Thread
    // Termination handled by rx2 closing when tx2 is dropped.
    let render_thread = thread::spawn(move || {
        while let Ok(unit) = rx2.recv() {
            let mut output = render_stage(unit);
            // EndTime MUST be recorded immediately after RenderStage returns
            output.end_time_ms = get_now_ms();
            log_unit(&mut log_file, "A", &output);
        }
    });

    // ProcessStage Thread
    thread::spawn(move || {
        while let Ok(unit) = rx1.recv() {
            let output = process_stage(unit);
            tx2.send(output).expect("Process -> Render send failed");
        }
        // tx2 dropped here, signaling rx2
    });

    // FetchStage Thread (Entry)
    thread::spawn(move || {
        for base_item in dataset {
            for i in 0..repetitions {
                let mut unit = base_item.clone();
                unit.iteration = i;

                // StartTime MUST be recorded immediately before first FetchStage call
                unit.start_time_ms = get_now_ms();
                let output = fetch_stage(unit);

                tx1.send(output).expect("Fetch -> Process send failed");
            }
        }
        // tx1 dropped here, signaling rx1
    });

    render_thread.join().expect("Render thread panicked");
}

/// --- CONFIGURATION B: SINGLE EXECUTION FLOW ---

fn run_config_b(dataset: Vec<ContentBuffer>, repetitions: usize) {
    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("EXP-ARCH-BASE-RUN-B.log")
        .expect("Cannot open log B");

    for base_item in dataset {
        for i in 0..repetitions {
            let mut unit = base_item.clone();
            unit.iteration = i;

            // StartTime MUST be recorded immediately before first FetchStage call
            unit.start_time_ms = get_now_ms();

            // Execute stages in a single linear call chain
            let f = fetch_stage(unit);
            let p = process_stage(f);
            let mut r = render_stage(p);

            // EndTime MUST be recorded immediately after RenderStage returns
            r.end_time_ms = get_now_ms();

            log_unit(&mut log_file, "B", &r);
        }
    }
}

/// --- EXECUTION ENTRY ---

fn main() {
    let args: Vec<String> = env::args().collect();
    let mode_arg = args
        .iter()
        .position(|r| r == "--mode")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str());

    // Dataset MUST be pre-generated before execution starts
    let mut dataset = Vec::new();
    let ids = vec!["alpha", "beta", "gamma", "delta", "epsilon"];
    for id in ids {
        dataset.push(ContentBuffer {
            input_id: id.to_string(),
            iteration: 0,
            payload: format!("Payload content for identity validation: {}", id).into_bytes(),
            status: Status::SUCCESS,
            start_time_ms: 0,
            end_time_ms: 0,
        });
    }

    // Each input MUST be processed exactly 1000 times per config
    let repetitions = 1000;

    match mode_arg {
        Some("A") => {
            run_config_a(dataset, repetitions);
        }
        Some("B") => {
            run_config_b(dataset, repetitions);
        }
        _ => {
            eprintln!("Usage: benchmark --mode [A|B]");
            std::process::exit(1);
        }
    }
}
