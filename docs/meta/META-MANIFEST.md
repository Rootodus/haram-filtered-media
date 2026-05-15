# Architectural Manifest (Handover Document)
ID: META-MANIFEST  
Status: STABLE  
Depends on: @ARCH-REQ, @LOG-DECISIONS

## Project Principles
- Target: Native performance browser-like runtime.
- Priority: 1. End-to-End Latency, 2. Simplicity, 3. Throughput.

## Critical Nuances Discovered in Prototype Phase
- Data Plane: TCP Loopback throughput is sufficient (~770 MB/s), but sequential serialization is the primary bottleneck for structural data.
- Serialization: MessagePack is REJECTED for DOM/Metadata. FlatBuffers is MANDATORY to achieve O(1) random access and zero-decode speeds.
- Header-Payload Separation: High-bandwidth data (Pixels) MUST be sent as raw bitstreams trailing the structural FlatBuffer to bypass builder/encoder overhead.
- Memory Layout: Use `Arc<[u8]>` (boxed slices) instead of `Arc<Vec<u8>>` to eliminate double-pointer indirection and maximize CPU cache hits during ML inference.
- Pipeline Synchronization: The system uses Hard-Synchronous Stop-and-Wait backpressure. The `Loader` is clocked to the `Renderer`. The `0x01` ACK MUST only be sent after `surface_texture.present()`.
- Atomic Fast-Path: The `Renderer` MUST check an `AtomicBool` "dirty" flag before attempting to lock the `FrameState` `Mutex` to prevent UI thread stalls.
- Hardware Target: The Intel Iris Xe (Unified Memory Architecture) is the baseline. GPU uploads (`write_texture`) are highly efficient but memory bandwidth is shared with the CPU; avoid unnecessary string allocations.
- Trait Architecture: `winit 0.30` implementation MUST use the `ApplicationHandler` trait. `wgpu 29.0` initialization MUST occur within the `resumed()` hook.

## Strategic Red-Lines (What NOT to do)
- NO Sequential Scanning: Do not use any format (MessagePack, JSON) that requires the CPU to walk the buffer to find structural boundaries.
- NO Async for Compute: `tokio` is restricted to I/O (Fetcher/IPC). All Parsing, Inference, and Rendering MUST run on dedicated synchronous thread pools or the main thread.
- NO Double Indirection: Do not wrap pointers in pointers (e.g., `Arc<Vec<T>>`).
- NO Hidden Clones: Clones of `pixel_data` or large `DomNode` vectors in the hot-path are considered architectural failures.
- NO Model Network Access: The `MLProcessor` is physically isolated from networking crates to enforce security.

## Notes / Explanatory
- [EXPLANATORY] The transition from "Independent Threads" to "Hard-Sync" was necessitated by the observation of unmanaged latency drift in @EXP-SPIKE-03-VISUAL-WGPU.
- [EXPLANATORY] Performance metrics from @EXP-SPIKE-05-DOM-STRESS serve as the rejection criteria for any proposed non-zero-copy serialization.
