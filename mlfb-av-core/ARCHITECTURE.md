# `ARCHITECTURE.md` – `mlfb-av-core`
Last updated: 2026-07-24  
Status: Draft – changes require updating all affected tests/examples.

## Purpose
`mlfb-av-core` is a real‑time multimedia and ML execution engine. It processes audio (PCM) and video (RGBA8) streams from local files or SHM/IPC, applies chained ML transforms, and renders video to a `winit`/`wgpu` surface while feeding audio to the OS sound card via `cpal`.

## Non‑Negotiable Invariants
- Audio continuity is absolute. Under no condition may the audio callback block, allocate, or wait on a lock. Silence must be output when no processed audio is ready.
- Video may drop frames. If downstream queues (ML, GPU upload) are full, the ingest must discard the frame immediately—never block.
- Zero heap allocation after steady‑state startup. All buffers (audio/video slots, staging buffers, command encoders) are pre‑allocated. `malloc`/`free` (including implicit in `Vec` growth, `Box`, etc.) are forbidden in the hot paths (ingest, ML, audio callback, render loop).
- No `std::sync::Mutex` or `tokio`‑based synchronization in hot paths. Use lock‑free atomics, `parking_lot` only for low‑frequency setup/teardown, and SPSC/MPSC queues from `crossbeam` or `ringbuf`.

## Memory Model

### Slot Pools
- Audio: `N_A` slots, each `AUDIO_SLOT_SIZE` bytes (fixed, typically 4096 for 1024 `f32` frames).
- Video: `N_V` slots, each `VIDEO_SLOT_SIZE` bytes (fixed, e.g., 1920×1080×4 = 8.3 MB).
  `N_V` > `N_A` to absorb video bursts.

Each slot is `#[repr(C)]` and contains:
- `payload: [u8; SIZE]`
- `generation: AtomicU32` – increments on every allocation from the free list.
- `state: AtomicU8` – transitions:
  `0` FREE → `1` INGESTED → `2` ML_ACQUIRED → `3` ML_COMMITTED → `4` GPU_UPLOADED (video only) → `5` CONSUMED (audio only) → back to `0`.

### Index Queues (Lock‑free, fixed capacity)
All indices are packed as `(slot_index << 32) | generation`. Queues:
- `audio_free`, `video_free` – list of available slots (state 0).
- `audio_ingested`, `video_ingested` – ready for ML (state 1).
- `audio_ml_ready` – ML‑processed audio, ready for CPAL (state 3).
- `video_ml_ready` – ML‑processed video, ready for GPU upload (state 3).
- `video_gpu_upload_ready` – uploaded textures ready for rendering (state 4).

Audio output to CPAL: Uses a dedicated SPSC ring buffer (`ringbuf::SharedRb`) – not slot indices – to allow the callback to read raw PCM without dereferencing slots.

## Threading Model
| Thread / Role | Priority | Core Affinity (recommended) | Description |
| --- | --- | --- | --- |
| CPAL Audio Callback (OS‑invoked) | Highest (realtime) | Fixed core, isolate from others | Reads from SPSC `audio_output_queue`. Must complete <100µs. No locks, no allocations. |
| Video Ingest (Tokio tasks or dedicated threads) | Normal | Any | Reads SHM/files; writes to `video_free` → `video_ingested`. Drops if `video_free` full. |
| Audio Ingest (Tokio or dedicated) | Slightly above normal | Any | Reads PCM; pushes to `audio_free` → `audio_ingested`. Blocks if `audio_free` empty (should not happen with proper sizing). |
| ML Workers (pool, e.g., `tokio::spawn_blocking` or Rayon) | Lower than ingest | Separate from audio/render cores | Consume `audio_ingested` and `video_ingested`; process; push to `audio_ml_ready` / `video_ml_ready`. Variable latency allowed. |
| GPU Upload (dedicated thread) | Normal | Near render core | Consumes `video_ml_ready`; copies to staging buffer; submits copy command; pushes to `video_gpu_upload_ready`. |
| WGPU Render Loop (winit event loop) | Normal | Dedicated core (or same as upload) | Polls `video_gpu_upload_ready`; presents. May render stale frame if queue empty. |
| Ingestion Supervisors (Tokio) | Low | Any | Monitor SHM, file decoders; push to ingest queues. |

No separate “Audio Clock Thread”. The CPAL callback is the clock. ML workers write directly to the SPSC queue consumed by the callback.

## Backpressure & Drop Policy
- Audio: `audio_free` capacity must be sized to absorb the worst‑case ML latency + jitter. If it ever fills, it is a fatal error (panic or suspend ingestion). We pre‑size to 500ms of audio.
- Video: When `video_free` is full, the ingest thread drops the incoming frame, increments a counter, and continues. No blocking.
- ML queues: Bounded. If `audio_ml_ready` or `video_ml_ready` becomes full, the ML producer blocks – but that implies the upload or CPAL consumer is lagging. The video consumer (upload) can skip frames by popping multiple indices. The audio consumer (CPAL) must never block; it reads from the separate SPSC ring buffer, which is sized to absorb ML bursts without blocking.

## Zero‑Allocation & Memory Reuse
- All slot payloads are allocated once at startup (`Vec<AudioSlot>` and `Vec<VideoSlot>`).
- Staging buffers for WGPU (`wgpu::Buffer`) are created with `mapped_at_creation: true` and reused. Unmap/map cycles do not allocate.
- Command encoders are reused per frame or created from a pool.
- No `String`, `Vec`, `Box` in hot paths. Use fixed‑size arrays or `arrayvec`.

## Fault Isolation & Shutdown
- Global `AtomicBool` `SHUTDOWN` set with `Release` ordering.
- All threads check it at loop top.
- `catch_unwind` on each worker thread. On panic:
  - Log error via `tracing`.
  - Drain relevant queues to prevent deadlocks.
  - Send shutdown signal to the WGPU loop via `winit::event_loop::EventLoopProxy`.
- The WGPU loop exits cleanly; CPAL stream is dropped, releasing device.
- No thread poisoning – use `parking_lot` to avoid poisoning, and atomics for coordination.

## Platform Abstraction
- OS‑specific thread priorities are isolated in `src/priority.rs` using `#[cfg(target_os = "...")]`.
- SHM ingestion uses `shared_memory` crate with conditional compilation for Windows/Linux/macOS (if supported).
- All other code is platform‑agnostic.

## External Dependencies – Interface Boundaries
- `cpal` – only in `src/audio/output.rs`.
- `wgpu` – only in `src/render/` and `src/upload.rs`.
- `tokio` – only in `src/ingest/` and `src/supervisor.rs`; never used in audio callback, render, or upload threads.
- `onnxruntime` / `ort` – only in `src/ml/`; exposes `process_slice(&mut [u8]) -> Result<()>`.
- `crossbeam` / `ringbuf` – used for queues; all queue types are wrapped in private modules to prevent leaking types.

## Verification & Testing
- Every invariant listed here must have a corresponding test or example in `examples/` or `tests/`.
- The CI pipeline must run:
  - `cargo test` (unit + integration)
  - `cargo run --example audio_callback` (must complete under 100µs)
  - `cargo run --example slot_pool` (loom stress)
  - `cargo run --example wgpu_staging` (dhat no leak)
  - `cargo run --example integration_load` (hdrhistogram p99 latency < 20ms for video, audio jitter < 1ms)
- Any change to this document requires updating the corresponding tests.
