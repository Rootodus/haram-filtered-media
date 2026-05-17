# System Map (Data Flow Specification)
ID: ARCH-SYS-MAP  
Status: STABLE  
Depends on: @ARCH-REQ, @STD-DOC

## Data Flow Pipeline
The system operates as an asynchronous multithreaded pipeline within a single monolithic native process. Data transitions through the following stages.

### 1. Acquisition Stage - STAGE-ACQUIRE
- Mechanic: `Loader` (Headless Chrome Sidecar) generates snapshots based on @ARCH-REQ::SNAPSHOT-TRIGGER.
- Transport: Data moves via @ARCH-REQ::IPC-FLATBUFFERS over a binary pipe into the native process memory.
- Output: `RawBuffer` (Length-prefixed FlatBuffer containing DOM/Metadata + Trailing raw pixel bitstream).

### 2. Transformation Stage - STAGE-TRANSFORM
- Parser: Verifies the memory-mapped FlatBuffer within `RawBuffer` and provides direct accessor roots for the DOM tree and styles.
- Metadata: Appends viewport-relative element coordinates.
- Unit Creation: Encapsulates the verified FlatBuffer and pixel slice into a `ContentBuffer`.
- Memory Strategy: The `ContentBuffer` IS wrapped in an `Arc<T>` (Atomic Reference Counted pointer). DATA-ARC

### 3. Extraction Stage - STAGE-EXTRACT
- Selector Application: Applies User-Provided Selectors (@ARCH-REQ::PLUGIN-DECLARATIVE) to the `Arc<ContentBuffer>`.
- Pruning: Only the selected DOM/Style nodes are retained for feature synthesis.
- Mapping: Maps pruned data to fixed-width numerical features.
- Output: `InferenceTensor` (Contiguous memory block).

### 4. Inference Stage - STAGE-INFER
- MLProcessor: Ingests `InferenceTensor` into `ONNX Runtime`.
- Execution: Invokes model via GPU (Primary) or CPU (Fallback) (@ARCH-REQ::GPU-PRIORITY).
- Output: `ProcessedBuffer` containing the following instructions. DATA-INSTRUCTIONS
  - Temporal Instructions: Audio mutes/frequency shifts (Stream Layer).
  - Spatial Instructions: Blur/Pixelate/Blackbox masks (Stream Layer).
  - Textual Instructions: String replacement maps (Content Layer).
- Constraint: The original `ContentBuffer` remains UNCHANGED and read-only.

### 5. Finalization Stage - STAGE-RENDER
- Composition: `Renderer` (wgpu) receives the original `Arc<ContentBuffer>` AND the `ProcessedBuffer`.
- STREAM-LAYER: Applies coordinate-based pixel masks AND temporal audio segments.
- Content Layer: Applies text replacements AND performs required DOM reflow.
- Input Proxying: Captures user events (Clicks/Keys) and proxies them back to the `Loader` via `CDP`.

## Memory Management Strategy

### Zero-Copy Pointer Passing - MEM-ZEROCOPY
- Data MUST NOT be duplicated between pipeline stages.
- STAGE-TRANSFORM owns the allocation of the `ContentBuffer`.
- Stages (EXTRACT, INFER, RENDER) all reference the same memory address via `Arc`.
- Logic: Eliminates the serialization cost observed in extensions.

### Admission Control - MEM-ADMISSION
- The pipeline utilizes a Bounded Task Queue with a capacity of 1 between @STAGE-TRANSFORM and @STAGE-EXTRACT.
- Overwrite Policy: If the `MLProcessor` is busy, the pending `Arc<ContentBuffer>` is dropped AND replaced by the newest arrival.

## Component Interaction Map
| Transition | Mechanism | Responsibility |
| --- | --- | --- |
| `Loader` -> `Parser` | @ARCH-REQ::IPC-FLATBUFFERS Pipe | External Data Acquisition |
| `Parser` -> `Extractor` | `Arc<ContentBuffer>` | Feature Synthesis |
| `Extractor` -> `Inference` | `&[f32]` (Tensor View) | Model Input |
| `Inference` -> `Renderer` | `ProcessedBuffer` | Transformation Orders |
| `Renderer` -> `Loader` | `CDP` (JSON/WebSocket) | Input Event Proxying |

## Notes / Explanatory
- [EXPLANATORY] @STAGE-TRANSFORM is computationally trivial under the FlatBuffer model, as "parsing" is replaced by a non-recursive integrity check and pointer assignment.
- [EXPLANATORY] @STAGE-EXTRACT utilizes user-defined selectors to prevent "Mapping Bloat," ensuring only high-signal data enters the ML engine.
- [EXPLANATORY] The `Renderer` -> `Loader` feedback loop ensures the "Static Snapshot" remains interactive for the end user.
- [EXPLANATORY] @MEM-ZEROCOPY is achieved cross-process by FlatBuffers (eliminating decode-side allocation) and in-process via `Arc` (eliminating move-side copying).
