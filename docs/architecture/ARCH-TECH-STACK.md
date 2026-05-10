# Technology Stack
ID: ARCH-TECH-STACK  
Status: STABLE  
Depends on: ARCH-REQ, STD-DOC

## Component Mapping [Using: ARCH-REQ::*]

### Loader and IPC [Ref: DYN-SNAPSHOT, IPC-MSGPACK]
- `chromiumoxide`: CDP integration and Headless Chrome orchestration.
- `rmp-serde`: MessagePack implementation for binary serialization.
- `tokio`: Async runtime for I/O-bound acquisition tasks.

### Processing Pipeline [Ref: PIPE-MONOLITH, UNIT-BUFFER]
- `crossbeam-channel`: High-performance multi-threaded data passing.
- `rayon`: Work-stealing thread pool for compute-bound parsing and inference.
- `Arc<T>`: Standard library atomic reference counting for zero-copy memory sharing.

### Parsing and Mapping [Ref: UNIT-BUFFER, MAPPING]
- `scraper`: HTML parsing and CSS selector execution.
- `lightningcss`: High-performance CSS lexing and computed style resolution.

### Inference Engine [Ref: INFERENCE-BACKEND]
- `ort`: ONNX Runtime wrapper for hardware-accelerated inference.
- `ndarray`: Tensor manipulation and data preparation.

### Graphics and UI [Ref: WINDOW-OWNERSHIP, RENDER-LAYERED]
- `winit`: Cross-platform window creation and event loop.
- `wgpu`: Hardware-accelerated graphics API for the layered renderer.
- `egui`: Minimal immediate-mode GUI for the browser shell.
- `cosmic-text`: Multi-line text shaping and layout for the Content Layer.

### Audio Stream [Ref: AUDIO-CAPTURE]
- `cpal`: Low-level audio I/O for temporal stream modifications.

## Other Potential Useful Crates
- `smart_default`
- `derivative`

## Selection Rationale [Ref: LOG-DECISIONS]
- MessagePack over Protobuf: Selected to minimize build-system complexity while maintaining near-native serialization speeds.
- `ort` over `tract`: Selected to ensure access to GPU Execution Providers (CUDA/CoreML), which is required to satisfy `ENV-EXT-LATENCY`.
