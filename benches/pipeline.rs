use criterion::{Criterion, criterion_group, criterion_main};
use ml_filtered_browser::content_buffer::ContentBuffer;
use ml_filtered_browser::fetcher::fetch_stage;
use ml_filtered_browser::ml_processor::process_stage;
use ml_filtered_browser::renderer::render_stage;
use std::hint::black_box;

fn bench_pipeline(c: &mut Criterion) {
    c.bench_function("pipeline_core", |b| {
        b.iter(|| {
            let buf = black_box(ContentBuffer::new_dummy());
            let buf = fetch_stage(buf);
            let buf = process_stage(buf);
            let _ = render_stage(buf);
        });
    });
}

fn benchmark_large_payload(c: &mut Criterion) {
    let sizes = [1024 * 1024, 5 * 1024 * 1024];

    for size in sizes {
        c.bench_function(&format!("payload_{}", size), |b| {
            b.iter(|| {
                let buffer = fetch_large(size);
                let buffer = process_stage(buffer);
                let _ = render_stage(buffer);
            });
        });
    }
}

fn fetch_large(size: usize) -> ContentBuffer {
    let data = vec![0u8; size];
    ContentBuffer::from_bytes(data)
}

criterion_group!(benches, bench_pipeline, benchmark_large_payload);

criterion_main!(benches);
