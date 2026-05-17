# Requirements
ID: ARCH-REQ  
Status: STABLE  
Depends on: @STD-DOC

## Facts (Hard Constraints & Observations)
- ENV-EXT-LATENCY: ML execution in browser extensions introduces unacceptable overhead due to IPC serialization AND main-thread contention.
- NET-GET-ONLY: Modern network interactions allow complex mutations, but this system environment is restricted to static retrieval to minimize state complexity.
- DYN-WEB-JS: Modern websites REQUIRE JavaScript for functional content resolution, necessitating a high-fidelity engine for acquisition.
- COMP-TRADE: High-fidelity document rendering AND high-speed ML inference are computationally competitive goals requiring distinct resource allocation.
- PLATFORM-DESKTOP: Mobile operating systems prohibit the required sidecar process architecture; the system is restricted to Windows, Linux, and macOS.
- GPU-CONTENTION: Simultaneous execution of the `Loader` (Chrome), `Renderer` (wgpu), and `MLProcessor` (ONNX) creates high VRAM pressure AND compute contention.

## Decisions (Committed Architecture)
- HOST-NATIVE: The system SHALL run as a standalone native process to minimize environment-induced latency.
- SCOPE-RESTRICTED: The system IS a restricted runtime; it IS NOT a full web browser.
- UI-SINGLE-TAB: The interface IS a minimal single-tab shell containing an address bar, navigation (Back/Forward), and reload functionality ONLY.
- NET-RESTRICT: Network interaction IS limited to `HTTP` `GET`. `POST`, `PUT`, and `DELETE` are PROHIBITED.
- DYN-SNAPSHOT: Dynamic content MUST be resolved into a static snapshot via an external `Loader` (Headless Chrome).
- SNAPSHOT-TRIGGER: The `Loader` SHALL emit a snapshot on: 1. Navigation complete, 2. DOM mutation idle for > 200 ms, or 3. User interaction event.
- IPC-FLATBUFFERS: Data transition between the `Loader` and the native runtime SHALL use `FlatBuffers` over a binary pipe (Unix Domain Sockets or Named Pipes) to enable zero-decode random access (@SPEC-ML-PROC::PROTOCOL-SPIKE).
- PIPE-MONOLITH: The core pipeline (Ingest -> Parse -> Infer -> Render) SHALL execute within a single monolithic native process using Multithreading for stage isolation to enable zero-copy data passing.
- MODE-SUPPORT: The system SHALL support two `ExecutionMode` values: `latency` AND `throughput`.
- PLUGIN-DECLARATIVE: Users SHALL insert ML models via a JSON manifest defining:
  - Input Selector: Data slice for model ingestion (CSS selector or Text regex).
  - Logic Mapping: Output threshold triggers for specific actions.
  - Action Trigger: Selection from a pre-defined native Action Library.
- ACTION-LIBRARY: The runtime SHALL provide a fixed library of actions:
  - Destructive: Blur, Blackbox, Pixelate, Mute-Audio, Change-Audio-Frequency, Hide-Element.
  - Semantic: Replace-Text.
- RENDER-LAYERED: The `Renderer` SHALL utilize a two-layer model:
  - Stream Layer: Handles time-series bitstreams (Video, Audio, Images).
    - Visuals: Modified via coordinate-based masks.
    - Audio: Modified via temporal segments (time-stamps).
    - Constraint: PROHIBITED from triggering DOM reflow.
  - Content Layer: Handles DOM/Text; PERMITTED to trigger reflow for semantic replacements.
- GPU-PRIORITY: The `Renderer` SHALL have priority for VRAM AND GPU compute. If VRAM headroom is < 10% or latency spikes, the `MLProcessor` MUST throttle or fallback to CPU inference.
- INPUT-PROXYING: The `Renderer` SHALL capture mouse/keyboard events AND proxy them back to the `Loader` via `CDP` to maintain site interactivity.
- SEC-NETWORK-ISOLATION: The `MLProcessor` module SHALL NOT have access to networking crates or system network interfaces to prevent data exfiltration.
- SEC-OS-BOUNDARY: The primary security boundary IS the host OS process isolation; user-inserted models are executed at the user's own risk.
- UNIT-CONTENTBUFFER: The unit of processing IS a `ContentBuffer` containing a serialized DOM snapshot, computed CSS styles, and viewport-relative element coordinates.
- MODEL-ROUTING: The system SHALL activate models based on a user-defined URL-pattern-to-Model mapping manifest.
- WINDOW-OWNERSHIP: The native process SHALL create and own the OS window via a hardware-accelerated graphics library (e.g., `winit` + `wgpu`).
- LOADER-LIFECYCLE: The native process SHALL manage the `Loader` sidecar as a child process.
- INFERENCE-BACKEND: The system SHALL utilize the `ONNX Runtime` with execution providers prioritized as: 1. GPU (CUDA/DirectML/CoreML) 2. CPU.
- ASYNC-ACQUISITION: The `Fetcher` and `Loader-Bridge` SHALL use `tokio` async tasks. Inference and Rendering SHALL use dedicated synchronous thread-pools to prevent executor starvation.

## Gaps (Active Blockers)
- MAPPING: The specific algorithm for ordering, truncating, and encoding DOM nodes into fixed‑width tensor indices IS NOT defined. Absolute pixel coordinates are available via `Rect`.
- THRESHOLD: The numerical trigger conditions for the system to override user-defined `ExecutionMode` preferences are NOT defined.
- AUDIO-CAPTURE: The mechanism for capturing raw audio buffers from the `Loader` (Chrome) into the native `Stream Layer` is NOT defined.
- RESOURCE-QUOTA: It is not defined how the system prevents a user-inserted model from consuming 100% of the System RAM or CPU.
- SPEC-PARSER: Mechanical specification for tokenizing HTML/CSS into tensors is pending model selection.
- SPEC-INPUT-PROXY: Mechanical specification for coordinate-mapped event propagation via CDP is pending renderer stabilization.
- SPEC-ML-CORE: Mechanical specification for `ort` (ONNX) session management and thread-pool isolation is pending pipeline integration.

## Implementation Criteria
- NATIVE-OVER-EXTENSION: Rejection of browser extensions is based on IPC serialization bottlenecks and JS main-thread contention. Any proposed solution involving high-frequency data copying or JS-side logic is a regression.
- TCP-JITTER-CONTROL: TCP `nodelay` MUST be enabled on both sides of the IPC pipe. The system priorities low-jitter real-time delivery over bulk bandwidth efficiency.
- GPU-PREEMPTION: The `Renderer` (wgpu) owns the GPU context. `MLProcessor` (ONNX) tasks are guest operations and MUST be throttled or offloaded to CPU if VRAM headroom is <10% to prevent UI stutter.
- ZERO-DECODE-CONTRACT: The IPC layer MUST NOT perform sequential scanning of the DOM tree. Data access MUST be performed via pointer offsets into memory-mapped FlatBuffer regions.

## Notes / Explanatory
- [EXPLANATORY] `FlatBuffers` was adopted as the primary IPC format to resolve the O(N) sequential scanning bottleneck observed with `MessagePack` in @EXP-SPIKE-05-DOM-STRESS, which exceeded the 16.6 ms frame budget for 5,000 nodes.
- [EXPLANATORY] `Raw Pixels` remain a trailing unencoded bitstream following the `FlatBuffer` metadata to avoid the overhead of structural wrapping for bulk binary data.
- [EXPLANATORY] The `Single-Tab` constraint simplifies memory management and ensures maximum CPU cache locality for the active task.
- [EXPLANATORY] Async is utilized ONLY for I/O-bound tasks (Fetcher/Networking); dedicated thread pools are utilized for compute-bound tasks (Inference/Parsing) to prevent executor starvation.
- [EXPLANATORY] PIPE-MONOLITH utilizes `Arc<T>` for internal data passing to ensure zero-copy performance between threads.
- [IDEA] Semantic summarization of elements remains a candidate feature but is currently excluded from ACTION-LIBRARY due to reflow performance costs.
