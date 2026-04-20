# System Map (Data Flow Specification)
ID: ARCH-SYS-MAP  
Status: STABLE  
Depends on: ARCH-REQ, STD-DOC

## Data Flow Pipeline
The system operates as a unidirectional pipeline. Data transitions through the following stages within a single memory space.

### 1. Acquisition Stage [STAGE-ACQUIRE]
- Source A [Static]: `Fetcher` performs a direct `HTTP` `GET` request. Output is raw `HTML` bytes.
- Source B [Dynamic]: `Loader` [Headless Chrome] generates a serialized `DOM` snapshot AND computed styles.
- Output: `RawBuffer` [Bytes or JSON-serialized CDP snapshot].

### 2. Transformation Stage [STAGE-TRANSFORM]
- Parser: Converts `RawBuffer` into a structured, in-memory `DOM` tree AND `StyleMap`.
- Unit Creation: Encapsulates the structured data into a `ContentBuffer`.
- Memory Strategy [DATA-ARC]: The `ContentBuffer` IS wrapped in an `Arc<T>` [Atomic Reference Counted pointer].

### 3. Extraction Stage [STAGE-EXTRACT]
- Feature Extractor: Traverses the `ContentBuffer` via the `Arc` pointer.
- Mapping: Maps DOM elements AND CSS properties to fixed-width numerical features [Tensors].
- Output: `InferenceTensor` [Contiguous memory block].

### 4. Inference Stage [STAGE-INFER]
- MLProcessor: Ingests the `InferenceTensor`.
- Execution: Invokes the ML model [ONNX/Tract].
- Output [DATA-INSTRUCTIONS]: `ProcessedBuffer` containing Transformation Instructions [e.g., Blur Masks, Redaction Rectangles, Text Replacement Maps].
- Constraint: The original `ContentBuffer` remains UNCHANGED and read-only.

### 5. Finalization Stage [STAGE-RENDER]
- Renderer: Receives the original `Arc<ContentBuffer>` AND the `ProcessedBuffer`.
- Composition: Applies the Transformation Instructions to the source data for final output.
- Modifications: Execution of blurs, black boxes, or text rewriting happens ONLY in this stage.

## Memory Management Strategy

### Zero-Copy Pointer Passing [MEM-ZEROCOPY]
- Data MUST NOT be duplicated between pipeline stages.
- `STAGE-TRANSFORM` owns the allocation of the `ContentBuffer`.
- All subsequent stages receive the same read-only `Arc<ContentBuffer>`.
- Logic: Modifications are represented as metadata overlays [Instructions] rather than byte-level mutations of the source.

### Admission Control [MEM-ADMISSION]
- The pipeline utilizes a Bounded Task Queue with a capacity of 1 between `STAGE-TRANSFORM` and `STAGE-EXTRACT`.
- Overwrite Policy: If the `MLProcessor` is busy, the pending `Arc<ContentBuffer>` IS dropped AND replaced by the newest arrival to ensure real-time latency [Ref: `MODE-LATENCY`].

## Component Interaction Map
| Transition | Mechanism | Responsibility |
| --- | --- | --- |
| `Loader` -> `Parser` | Bytes/CDP Pipe | Data Ingestion |
| `Parser` -> `Extractor` | `Arc<ContentBuffer>` | Feature Synthesis |
| `Extractor` -> `Inference` | `&[f32]` (Tensor View) | Model Input |
| `Inference` -> `Renderer` | `ProcessedBuffer` | Transformation Orders |
| `ContentBuffer` -> `Renderer` | `Arc<ContentBuffer>` | Source Material |

## Notes / Explanatory
- [EXPLANATORY] `MEM-ZEROCOPY` prevents the performance collapse seen in browser extensions by ensuring the 4MB-10MB buffer is never copied.
- [EXPLANATORY] `DATA-INSTRUCTIONS` allows for multi-model parallelism: multiple models can read the same `Arc<ContentBuffer>` simultaneously and produce independent instruction sets for the `Renderer`.
- [EXPLANATORY] `STAGE-RENDER` enables lazy modification; visual changes like blurring are performed at the final display step, potentially utilizing hardware acceleration.
