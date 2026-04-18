use std::sync::mpsc::sync_channel;
use std::thread;
use std::time::{Duration, Instant};

// --- CORE SYSTEM ---

#[derive(Clone, Debug)]
pub enum Status {
    SUCCESS,
    FAIL,
}

pub struct ContentBuffer {
    pub payload: Vec<u8>,
    pub status: Status,
}

pub fn fetch_stage(mut input: ContentBuffer) -> ContentBuffer {
    if input.payload.is_empty() {
        input.status = Status::FAIL;
    }
    input
}

pub fn process_stage(mut input: ContentBuffer) -> ContentBuffer {
    if let Status::SUCCESS = input.status {
        input.payload.reverse();
    }
    input
}

pub fn render_stage(input: ContentBuffer) -> ContentBuffer {
    input
}

// --- METRICS STRUCTURE ---

struct StrategyMetrics {
    mode: &'static str,
    total_duration: Duration,
    p99_latency_ns: u128,
    throughput: f64,
}

fn calculate_p99(mut latencies: Vec<u128>) -> u128 {
    latencies.sort_unstable();
    let index = (latencies.len() * 99 / 100).min(latencies.len() - 1);
    latencies[index]
}

// --- EXECUTION STRATEGIES ---

fn benchmark_sync(dataset: &[ContentBuffer], reps: usize) -> StrategyMetrics {
    let mut latencies = Vec::with_capacity(dataset.len() * reps);
    let start = Instant::now();
    for _ in 0..reps {
        for base in dataset {
            let item = ContentBuffer {
                payload: base.payload.clone(),
                status: Status::SUCCESS,
            };
            let t0 = Instant::now();
            let _ = render_stage(process_stage(fetch_stage(item)));
            latencies.push(t0.elapsed().as_nanos());
        }
    }
    let total_duration = start.elapsed();
    StrategyMetrics {
        mode: "SYNC",
        total_duration,
        p99_latency_ns: calculate_p99(latencies),
        throughput: (dataset.len() * reps) as f64 / total_duration.as_secs_f64(),
    }
}

fn benchmark_pipeline(dataset: &[ContentBuffer], reps: usize) -> StrategyMetrics {
    let (tx_f, rx_f) = sync_channel::<ContentBuffer>(128);
    let (tx_p, rx_p) = sync_channel::<ContentBuffer>(128);
    let (tx_r, rx_r) = sync_channel::<ContentBuffer>(128);
    let (tx_out, rx_out) = sync_channel::<(Instant, bool)>(128);

    thread::spawn(move || {
        while let Ok(it) = rx_f.recv() {
            let _ = tx_p.send(fetch_stage(it));
        }
    });
    thread::spawn(move || {
        while let Ok(it) = rx_p.recv() {
            let _ = tx_r.send(process_stage(it));
        }
    });
    thread::spawn(move || {
        while let Ok(it) = rx_r.recv() {
            let _ = render_stage(it);
            let _ = tx_out.send((Instant::now(), true));
        }
    });

    let h_collector = thread::spawn(move || {
        let mut lats: Vec<u128> = Vec::new();
        while let Ok((_, _)) = rx_out.recv() { /* Placeholder for individual latency tracking if needed */
        }
        lats
    });

    // For PIPELINE, we track the t0 of each item explicitly for P99
    let (tx_f_timed, rx_f_timed) = sync_channel::<(ContentBuffer, Instant)>(128);
    let (tx_out_timed, rx_out_timed) = sync_channel::<u128>(128);

    // Override threads for precise P99 measurement
    let h1 = thread::spawn(move || {
        let (tx_p, rx_p) = sync_channel::<(ContentBuffer, Instant)>(128);
        let (tx_r, rx_r) = sync_channel::<(ContentBuffer, Instant)>(128);

        thread::spawn(move || {
            while let Ok((it, t0)) = rx_f_timed.recv() {
                let _ = tx_p.send((fetch_stage(it), t0));
            }
        });
        thread::spawn(move || {
            while let Ok((it, t0)) = rx_p.recv() {
                let _ = tx_r.send((process_stage(it), t0));
            }
        });
        thread::spawn(move || {
            while let Ok((it, t0)) = rx_r.recv() {
                let _ = render_stage(it);
                let _ = tx_out_timed.send(t0.elapsed().as_nanos());
            }
        });
    });

    let h_lats = thread::spawn(move || {
        let mut lats = Vec::new();
        while let Ok(lat) = rx_out_timed.recv() {
            lats.push(lat);
        }
        lats
    });

    let start = Instant::now();
    for _ in 0..reps {
        for base in dataset {
            let _ = tx_f_timed.send((
                ContentBuffer {
                    payload: base.payload.clone(),
                    status: Status::SUCCESS,
                },
                Instant::now(),
            ));
        }
    }
    drop(tx_f_timed);
    let latencies = h_lats.join().unwrap();
    let total_duration = start.elapsed();

    StrategyMetrics {
        mode: "PIPELINE",
        total_duration,
        p99_latency_ns: calculate_p99(latencies),
        throughput: (dataset.len() * reps) as f64 / total_duration.as_secs_f64(),
    }
}

// --- MAIN DECISION ENGINE ---

fn main() {
    let sizes = [1024, 8192, 131072, 1024, 8192];
    let dataset: Vec<ContentBuffer> = sizes
        .iter()
        .map(|&s| ContentBuffer {
            payload: vec![0u8; s],
            status: Status::SUCCESS,
        })
        .collect();

    // Warmup
    for base in &dataset {
        let _ = render_stage(process_stage(fetch_stage(ContentBuffer {
            payload: base.payload.clone(),
            status: Status::SUCCESS,
        })));
    }

    println!("--- ARCHITECTURE SELECTION REPORT ---");

    let sync_res = benchmark_sync(&dataset, 1000);
    let pipe_res = benchmark_pipeline(&dataset, 1000);

    println!(
        "{:<10} | {:<12} | {:<12} | {:<12}",
        "MODE", "WALL TIME", "THROUGHPUT", "P99 LATENCY"
    );
    println!("{:-<55}", "-");

    for res in &[&sync_res, &pipe_res] {
        println!(
            "{:<10} | {:<11.4}s | {:<9.2} i/s | {:<10} ns",
            res.mode,
            res.total_duration.as_secs_f64(),
            res.throughput,
            res.p99_latency_ns
        );
    }

    println!("\n--- DECISION LOGIC ---");
    if pipe_res.throughput > sync_res.throughput * 1.2 {
        println!("CHOICE: PIPELINE. Throughput gain (>20%) justifies concurrency overhead.");
    } else if pipe_res.p99_latency_ns > sync_res.p99_latency_ns * 2 {
        println!(
            "CHOICE: SYNC. Pipeline introduces excessive jitter (P99) without sufficient throughput gain."
        );
    } else {
        println!("CHOICE: SYNC. Simplicity is preferred when performance delta is marginal.");
    }
}
