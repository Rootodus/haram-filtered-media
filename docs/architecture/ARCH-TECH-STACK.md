# Technology Stack
ID: ARCH-TECH-STACK  
Status: STABLE  
Depends on: @ARCH-REQ, @STD-DOC

## Component Mapping [Using: ARCH-REQ::*]

### Loader and IPC (@DYN-SNAPSHOT, @IPC-FLATBUFFERS)
- `chromiumoxide`: CDP integration and Headless Chrome orchestration.
- Node.js + Puppeteer used only for spike testing. Production target: Rust + chromiumoxide.
- `tokio`: Async runtime for I/O-bound acquisition tasks.
- `flatbuffers`: Zero-copy serialization library for structural data access.
- `flatc`: Schema compiler for generating Rust/JS data access code.

### Processing Pipeline (@PIPE-MONOLITH, @UNIT-CONTENTBUFFER)
- `crossbeam-channel`: High-performance multi-threaded data passing.
- `rayon`: Work-stealing thread pool for compute-bound parsing and inference.
- `Arc<T>`: Standard library atomic reference counting for zero-copy memory sharing.

### Parsing and Mapping (@UNIT-CONTENTBUFFER, @MAPPING)
- `scraper`: HTML parsing and CSS selector execution.
- `lightningcss`: High-performance CSS lexing and computed style resolution.

### Inference Engine (@INFERENCE-BACKEND)
- `ort`: ONNX Runtime wrapper for hardware-accelerated inference.
- `ndarray`: Tensor manipulation and data preparation.

### Graphics and UI (@WINDOW-OWNERSHIP, @RENDER-LAYERED)
- `winit`: Cross-platform window creation and event loop.
- `wgpu`: Hardware-accelerated graphics API for the layered renderer.
- `egui`: Minimal immediate-mode GUI for the browser shell.
- `cosmic-text`: Multi-line text shaping and layout for the Content Layer.

### Audio Stream (@AUDIO-CAPTURE)
- `cpal`: Low-level audio I/O for temporal stream modifications.

## Other Potential Useful Crates
- `smart_default`
- `derivative`

## Selection Rationale (@LOG-DECISIONS)
- FlatBuffers over MessagePack: Adopted to eliminate the 11–124 ms sequential scanning overhead observed in @EXP-SPIKE-05-DOM-STRESS. FlatBuffers allows the `MLProcessor` to access DOM nodes via memory-mapped offsets with zero CPU parsing. MessagePack is relegated to non-critical R&D tasks only.
- `ort` over `tract`: Selected to ensure access to GPU Execution Providers (CUDA/CoreML), which is required to satisfy @ARCH-REQ::ENV-EXT-LATENCY, and to maximize compatibility with user-provided models and allow seamless scaling from CPU fallback to GPU acceleration. Pure-Rust alternatives (like `tract`) are rejected due to limited operator support and lack of GPU execution providers.
