# ARCHITECTURE.md – haram-filtered-media (hfm-core)
Last updated: 2026-08-12  
Status: Living document – update only when core invariants change.

## Core Invariants (Non‑Negotiable)
1. **Audio priority**: The CPAL callback must be lock‑free, allocation‑free, and complete in < 100 µs. If the SPSC ring buffer is empty, it outputs silence – never blocks.
2. **Video backpressure**: Ingest must never block. If downstream queues (ML, upload) are full, video frames are dropped immediately. This guarantees the ingest thread stays responsive.
3. **Zero heap allocations in hot paths**: All audio/video buffers, staging buffers, and command encoders are pre‑allocated at startup. No `Vec` growth, `Box`, or `String` in the render, ML inference, or audio callback loops.
4. **No `std::sync::Mutex` in hot paths**: Use `parking_lot::Mutex` (non‑poisoning) or lock‑free queues (`crossbeam`). `std::sync::Mutex` is only allowed for setup/shutdown.

## Pipeline Overview
- **Ingest**: GStreamer (files) or Chromiumoxide (web) → raw RGBA frames (960×540).
- **SlotPool**: Fixed‑capacity memory pool. State transitions: `FREE → INGESTED → ML_COMMITTED → GPU_UPLOADED → FREE`. Protected by `parking_lot::Mutex`; lock‑free is not currently used.
- **ML Filter**: PPHumanSeg (ONNX, Apache 2.0).
  - Preprocess: Box‑filter downscale to 192×192 → planar RGB `f32`.
  - Inference: DirectML (Windows), ~8 ms per frame.
  - Postprocess: Fused nearest‑neighbour upscale + blackout.
- **Render**: `wgpu` upload and display.

## Platform & Backend
- **Windows**: DirectML (primary, no CPU fallback).
- **Linux**: OpenVINO (GPU, fallback to CPU).
- **macOS**: CoreML (if needed, but not primary).

## Workspace Structure
- **Workspace**: `haram-filtered-media`
- **Crate prefix**: `hfm-`
- `hfm-core`: Library crate (shared pipeline, SlotPool, ML, renderer).
- `hfm-web`: Binary crate (headless browser source via `chromiumoxide`).
- `hfm-player`: Binary crate (standalone local media player, moved from examples).

## Performance Target
- Steady‑state frame time: < 20 ms (currently ~10–12 ms on Intel Iris Xe iGPU).

## Dependencies (Key)
- `ort` (with `directml` feature) for inference.
- `wgpu`/`winit` for rendering.
- `gstreamer` for audio/video ingest.
- `cpal` for audio output.
- `chromiumoxide` for web source.
