# Architectural Manifest (Handover Document)
ID: META-MANIFEST  
Status: STABLE

## Project Principles
- Target: Native performance browser-like runtime.
- Priority: 1. End-to-End Latency, 2. Simplicity, 3. Throughput.

## Critical Nuances Discovered in Prototype Phase
- Data Plane: TCP Loopback is currently sufficient (>700 MB/s), but jitter is the enemy.
- Memory: Arc<T> is mandatory for stage hand-offs. Clones are forbidden in the render path.
- Graphics: Winit 0.30 and Wgpu 29.0 are the target stack. ApplicationHandler trait is the required architecture.
- Mapping: User-defined CSS/Regex selectors are used to prune the DOM before tensor mapping to avoid "DOM-to-Tensor" bloat.
- Trait Architecture: `winit 0.30` implementation MUST use the `ApplicationHandler` trait. Do not use the deprecated `event_loop.run` closure.
- Coordinate Truth: The `Loader` (Chrome) is the source of truth for element positions. If `Content Layer` reflow causes visual drift, the `Renderer` MUST provide an offset-map for `INPUT-PROXYING`.
- Non-blocking IPC: The IPC/Socket thread MUST operate independently of the Render thread. The `SharedAppState` `Mutex` MUST be held for the minimum time possible to avoid pipeline stalls.

## Strategic Red-Lines (What NOT to do)
- Do not use Async for compute-bound ML inference.
- Do not use MessagePack for 8MB+ binary payloads.
- Do not allow ML models to access the network.
