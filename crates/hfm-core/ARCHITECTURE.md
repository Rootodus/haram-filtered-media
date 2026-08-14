# ARCHITECTURE.md – haram-filtered-media (hfm-core)
Last updated: 2026-08-14  
Status: Living document – update only when core invariants change.

## Core Invariants (Non‑Negotiable)
1. **Audio priority**: The CPAL callback must be lock‑free, allocation‑free, and complete in < 100 µs. If the SPSC ring buffer is empty, it outputs silence – never blocks.
2. **Video backpressure**: Ingest uses a blocking push with a retry loop. If downstream queues (ML, upload) are full, the ingest thread waits until space is available. Frames are **never dropped**; this ensures completeness at the cost of possible ingest slowdown.
3. **Heap allocations are minimised in hot paths**: Most critical loops reuse pre‑allocated buffers (e.g., `SlotPool`, `MediaBuffer`, staging buffers). However, a small number of bounded allocations are acceptable where they simplify logic and do not impact real‑time performance (e.g., the ML input tensor buffer of ~110k floats per frame). These are intentionally kept small and fixed‑size.
4. **No `std::sync::Mutex` in hot paths**: Use `parking_lot::Mutex` (non‑poisoning) or lock‑free queues (`crossbeam`). `std::sync::Mutex` is only allowed for setup/shutdown.

## Pipeline Overview
- **Ingest**: GStreamer (files) or Chromiumoxide (web) → raw RGBA frames (960×540).
- **SlotPool**: Fixed‑capacity memory pool. State transitions: `FREE → INGESTED → ML_COMMITTED → GPU_UPLOADED → FREE`. Protected by `parking_lot::Mutex`.
- **MediaBuffer**: A sliding window buffer that sits between the ML threads and the render/output threads. It stores processed video frames and audio chunks with PTS, supports fill‑level measurement for throttling, and handles seek flushing.
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
- `hfm-core`: Library crate (shared pipeline, SlotPool, MediaBuffer, ML, renderer).
- `hfm-web`: Binary crate (headless browser source via `chromiumoxide`).
- `hfm-player`: Binary crate (standalone local media player, moved from examples).

## Planned Features (Short‑Term, Subject to Change)
- **Text filtering**: Sentiment analysis on DOM text (already prototyped in web).
- **Audio ML**: Music removal / vocal isolation (ONNX model integration).
- **Web source integration**: Connect `chromiumoxide` screenshot capture to SlotPool.
- **Playback controls**: Seek, buffer, and frame cache (MediaBuffer already provides the foundation).
- **GUI**: Optional `egui` overlay for controls and debug information.

## Performance Target
- Steady‑state frame time: < 20 ms (currently ~10–12 ms on Intel Iris Xe iGPU).

## Dependencies (Key)
- `ort` (with `directml` feature) for inference.
- `wgpu`/`winit` for rendering.
- `gstreamer` for audio/video ingest.
- `cpal` for audio output.
- `chromiumoxide` for web source.
