IMPORTANT: This document became inaccurate shortly after it was created, so it needs to be updated or deleted.

# ARCHITECTURE.md – mlfb-av-core
Last updated: 2026-07-24  
Status: Draft – changes require updating all affected tests/examples.

## Constants
- AUDIO_SLOT_SIZE = 4096 (1024 f32, 21.3ms @ 48kHz)
- VIDEO_SLOT_SIZE = 1920\*1080\*4 (8.3 MB)
- N_A = 64
- N_V = 8
- AUDIO_OUTPUT_QUEUE_CAP = 16384 samples (170ms @ 48kHz)
- All queue capacities = N_A for audio, N_V for video.

## Invariants (must hold)
1. Audio callback: <100µs, no alloc, no lock, reads SPSC, outputs silence if empty.
2. Video ingest: if video_free full, drop frame, inc counter, continue.
3. No heap allocation after startup (all slots, staging, encoders pre‑allocated).
4. No std::sync::Mutex, no tokio sync in audio/upload/render.
5. CPAL callback is the clock – no separate audio thread.

## Slot Struct
#[repr(C)] struct Slot<T> { payload: [u8; SIZE], generation: AtomicU32, state: AtomicU8 }  
State transitions: 0 FREE → 1 INGESTED → 2 ML_ACQUIRED → 3 ML_COMMITTED → (4 GPU_UPLOADED video only) → 5 CONSUMED audio → 0.  
Atomic ordering: state.load Acquire, state.store Release, generation SeqCst.

## Queues (lock‑free, fixed capacity)
| Queue | Producer | Consumer | Drop on full |
| --- | --- | --- | --- |
| audio_free | CPAL SPSC writer (after copy) | Audio ingest | N/A (panic if empty) |
| video_free | Render loop (after present) | Video ingest | N/A – ingest drops frame |
| audio_ingested | Audio ingest | ML workers | Panic if full |
| video_ingested | Video ingest | ML workers | Drop frame |
| audio_ml_ready | ML workers | CPAL SPSC writer | Panic if full |
| video_ml_ready | ML workers | GPU upload | Drop video slot |
| video_gpu_upload_ready | GPU upload | Render loop | Render stale frame |

## Thread Roles
| Role | Priority | Affinity | I/O |
| --- | --- | --- | --- |
| CPAL callback | Realtime | Fixed core | Reads audio_output_queue |
| CPAL SPSC writer | High (just below realtime) | Separate core, high priority (but not realtime) | Consumes audio_ml_ready → writes SPSC → releases to audio_free |
| Audio ingest | High | Any | audio_free → audio_ingested |
| Video ingest | Normal | Any | video_free → video_ingested (drops if either full) |
| ML workers | Low | Separate cores | Consume audio_ingested/video_ingested; push to audio_ml_ready/video_ml_ready |
| GPU upload | Normal | Near render | video_ml_ready → staging → video_gpu_upload_ready |
| WGPU loop | Normal | Fixed core | video_gpu_upload_ready → present; after present, release slot to video_free |

## Drop Policies
- audio_free empty: panic! (or suspend ingestion).
- audio_ingested full: panic! – cannot drop audio.
- audio_ml_ready full: panic! – cannot drop processed audio.
- video_free full: drop frame.
- video_ingested full: drop frame (same check as above).
- video_ml_ready full: drop processed video slot.
- video_gpu_upload_ready full: render stale frame (consumer lags).

## Shutdown Order
1. Set SHUTDOWN atomic (Release).
2. Drop CPAL stream (unregisters callback).
3. Join all workers (they check SHUTDOWN at loop top).
4. Drop all queues (discard indices).
5. Drop WGPU device/surface.

## Platform Abstraction
- src/priority.rs: set_realtime(), set_normal() with cfg(target_os).
- src/shm.rs: uses shared_memory crate with conditional compilation.

## Dependency Boundaries
- cpal: src/audio/output.rs
- wgpu: src/render/, src/upload/
- tokio: src/ingest/, src/supervisor/ (only spawn_blocking, no async in hot path)
- ort: src/ml/
- ringbuf/crossbeam: private queue wrappers only

## Verification
- examples/audio_callback: <100µs, zero alloc
- examples/slot_pool: loom stress
- examples/wgpu_staging: dhat no leak
- examples/integration_load: hdrhistogram p99 <20ms video, audio jitter <1ms
- cargo test (unit + integration) must pass.
- Any change to this doc requires updating corresponding examples/tests.
