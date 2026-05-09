# System Map (Data Flow Specification)
ID: ARCH-SYS-MAP  
Status: STABLE  
Depends on: ARCH-REQ, STD-DOC

## Data Flow Pipeline
The system operates as an asynchronous multithreaded pipeline within a single monolithic native process. Data transitions through the following stages.

### 1. Acquisition Stage [Anchor: STAGE-ACQUIRE]
- Mechanic: `Loader` (Headless Chrome Sidecar) generates snapshots based on ARCH-REQ::SNAPSHOT-TRIGGER.
- Transport: Data moves via ARCH-REQ::IPC-MSGPACK over a binary pipe into the native process memory.
- Output: `RawBuffer` (MessagePack-encoded DOM/Style data).

### 2. Transformation Stage [Anchor: STAGE-TRANSFORM]
- Parser: Deserializes `RawBuffer` into a structured, in-memory `DOM` tree AND `StyleMap`.
- Metadata: Appends viewport-relative element coordinates.
- Unit Creation: Encapsulates structured data into a `ContentBuffer`.
- Memory Strategy [Anchor: DATA-ARC]: The `ContentBuffer` IS wrapped in an `Arc<T>` (Atomic Reference Counted pointer).

### 3. Extraction Stage [Anchor: STAGE-EXTRACT]
- Selector Application: Applies User-Provided Selectors [Ref: ARCH-REQ::PLUGIN-DECLARATIVE] to the `Arc<ContentBuffer>`.
- Pruning: Only the selected DOM/Style nodes are retained for feature synthesis.
- Mapping: Maps pruned data to fixed-width numerical features.
- Output: `InferenceTensor` (Contiguous memory block).

### 4. Inference Stage [Anchor: STAGE-INFER]
- MLProcessor: Ingests `InferenceTensor` into `ONNX Runtime`.
- Execution: Invokes model via GPU (Primary) or CPU (Fallback) [Ref: ARCH-REQ::GPU-PRIORITY].
- Output [Anchor: DATA-INSTRUCTIONS]: `ProcessedBuffer` containing:
  - Temporal Instructions: Audio mutes/frequency shifts (Stream Layer).
  - Spatial Instructions: Blur/Pixelate/Blackbox masks (Stream Layer).
  - Textual Instructions: String replacement maps (Content Layer).
- Constraint: The original `ContentBuffer` remains UNCHANGED and read-only.

### 5. Finalization Stage [Anchor: STAGE-RENDER]
- Composition: `Renderer` (wgpu) receives the original `Arc<ContentBuffer>` AND the `ProcessedBuffer`.
- Stream Layer: Applies coordinate-based pixel masks AND temporal audio segments.
- Content Layer: Applies text replacements AND performs required DOM reflow.
- Input Proxying: Captures user events (Clicks/Keys) and proxies them back to the `Loader` via `CDP`.

## Memory Management Strategy

### Zero-Copy Pointer Passing [Anchor: MEM-ZEROCOPY]
- Data MUST NOT be duplicated between pipeline stages.
- STAGE-TRANSFORM owns the allocation of the `ContentBuffer`.
- Stages (EXTRACT, INFER, RENDER) all reference the same memory address via `Arc`.
- Logic: Eliminates the serialization cost observed in extensions.

### Admission Control [Anchor: MEM-ADMISSION]
- The pipeline utilizes a Bounded Task Queue with a capacity of 1 between STAGE-TRANSFORM and STAGE-EXTRACT.
- Overwrite Policy: If the `MLProcessor` is busy, the pending `Arc<ContentBuffer>` IS dropped AND replaced by the newest arrival.

## Component Interaction Map
| Transition | Mechanism | Responsibility |
| --- | --- | --- |
| `Loader` -> `Parser` | ARCH-REQ::IPC-MSGPACK Pipe | External Data Acquisition |
| `Parser` -> `Extractor` | `Arc<ContentBuffer>` | Feature Synthesis |
| `Extractor` -> `Inference` | `&[f32]` (Tensor View) | Model Input |
| `Inference` -> `Renderer` | `ProcessedBuffer` | Transformation Orders |
| `Renderer` -> `Loader` | `CDP` (JSON/WebSocket) | Input Event Proxying |

## Notes / Explanatory
- [EXPLANATORY] STAGE-EXTRACT utilizes user-defined selectors to prevent "Mapping Bloat," ensuring only high-signal data enters the ML engine.
- [EXPLANATORY] The `Renderer` -> `Loader` feedback loop ensures the "Static Snapshot" remains interactive for the end user.
- [EXPLANATORY] MEM-ZEROCOPY is strictly enforced between threads within the monolith; cross-process copies only occur at the `Loader` boundary.
