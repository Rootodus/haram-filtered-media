# Decision Log
ID: LOG-DECISIONS  
Status: STABLE  
Depends on: @STD-DOC, @EXP-ARCH-BASELINE

## Decision: Read-only GET mode
Statement: The system boundary allows GET requests ONLY.  
Evidence: @ARCH-REQ::NET-RESTRICT.

Observed signals:
- Structural requirement to minimize state complexity in the monolithic pipeline.
- Reduced side-effect surface aligns with the "restricted execution runtime" goal.

Relationship mapping:
- Interaction surface restriction aligns with reduced implementation overhead.

Rejected alternatives:
- Allowing mixed HTTP methods with runtime filtering (higher observed divergence).

## [SUPERSEDED] Decision: Disable JS execution
- Status: SUPERSEDED by @DEC-JS-BOUNDARY.
- Reason: Conflicted with the requirement to support dynamic web content resolution via Headless Chrome.
- Reference: @ARCH-REQ::DYN-WEB-JS.

```historical_logic
Statement: JS execution is disabled in the content loading environment.  
Evidence: @EXP-ARCH-BASELINE.

Observed signals:
- Client-side execution introduces variability in rendered input representation.
- DOM mutation introduces inconsistent preprocessing states.

Relationship mapping:
- Removal of client-side execution aligns with more stable input extraction patterns.

Rejected alternatives:
- Sandboxed JS execution with partial allowlist (introduces timing variance).
```

## Decision: Pipeline architecture
Statement: System uses a staged (does NOT mean separate processes) pipeline architecture.  
Evidence: @EXP-ARCH-BASELINE.

Observed signals:
- Qualitative failure of IPC-based stage isolation (browser extensions/localhost servers).
- In-process multithreading avoids the serialization tax observed in initial prototypes.

Relationship mapping:
- Thread-based staging aligns with the Monolith requirement to eliminate IPC.

Rejected alternatives:
- Fully single-stage execution (loss of separation boundaries).

## Decision: Stateless MLProcessor
Statement: `MLProcessor` MUST NOT maintain persistent state between invocations.  
Evidence: @ARCH-REQ::PIPE-MONOLITH.

Observed signals:
- Ensures predictable execution regardless of call history, simplifying Hard-Sync logic.
- Aligns with standard ML inference patterns for frame-by-frame filtering.

Relationship mapping:
- Absence of cross-run state simplifies the Admission Control logic.

Rejected alternatives:
- Session-based state model (higher cross-run coupling observed).

## [SUPERSEDED] Decision: Async communication
- Status: SUPERSEDED by @DEC-HARD-SYNC-PIPE.
- Reason: Asynchronous decoupling caused unmanaged latency drift in @EXP-SPIKE-02-PIXEL-PIPE and @EXP-SPIKE-03-VISUAL-WGPU.
- References: @EXP-SPIKE-02-PIXEL-PIPE, @EXP-SPIKE-03-VISUAL-WGPU.

```historical_logic
Statement: Stages communicate via asynchronous bounded channels.  
Evidence: @EXP-ARCH-BASELINE.

Observed signals:
- Synchronous coupling increases upstream idle time.
- Queue-based buffering smooths throughput under burst conditions.

Relationship mapping:
- Decoupling via queues aligns with reduced blocking propagation.

Rejected alternatives:
- Direct synchronous chaining (backpressure amplification observed).
```

## Decision: Buffer sharing
Statement: Buffers use shared references where safe.  
Evidence: @ARCH-REQ::PIPE-MONOLITH.

Observed signals:
- Qualitative observation that deep copies of high-bandwidth data cause UI stutter.
- Shared references (`Arc<T>`) are required to meet the 16.6 ms frame budget.

Relationship mapping:
- Reduced copying aligns with the zero-copy performance goal.

Rejected alternatives:
- Deep copy per stage boundary (higher allocation cost observed).

## Decision: Documentation format
Statement: Standardize on atomic key/value formatting AND STE logic.  
Evidence: @ARCH-SYS-MAP structural evaluation.

Observed signals:
- Free-form blocks increase parsing ambiguity for AI models.
- Atomic key/value formatting reduces interpretation variance.

Relationship mapping:
- Structural constraints align with more consistent parsing behavior.

Rejected alternatives:
- Free-form documentation blocks (higher ambiguity observed).

## Decision: Reject MessagePack for Structural Data
Statement: The system SHALL NOT use MessagePack for DOM or metadata serialization.  
Evidence: @EXP-SPIKE-05-DOM-STRESS benchmarking.

Observed signals:
- 5,000-node DOM deserialization consumed 11-14 ms baseline.
- Jitter spikes up to 124 ms observed during sequential scanning.
- MessagePack requires O(N) traversal to identify field boundaries.

Relationship mapping:
- Elimination of sequential parsing aligns with 16.6 ms frame budget (Native Performance).

Rejected alternatives:
- Optimizing MessagePack via string-borrowing (proven statistically insufficient).

## Decision: Adopt FlatBuffers for IPC
Statement: The system SHALL use FlatBuffers for all hierarchical and metadata serialization.  
Evidence: @EXP-SPIKE-05-DOM-STRESS failure analysis.

Observed signals:
- MessagePack parsing overhead saturated CPU cycles required for GPU upload.
- Lack of random access prevented efficient "peeking" of DOM properties.

Relationship mapping:
- Transition to memory-mapped random access aligns with zero-decode goals.

Rejected alternatives:
- Apache Arrow (rejected due to columnar-to-row overhead for single-page inference).
- Cap'n Proto (rejected due to inferior JavaScript ecosystem support for the Loader).

## Decision: JS Execution Boundary - DEC-JS-BOUNDARY
Statement: JS execution IS PERMITTED within the `Loader` (Headless Chrome) sidecar ONLY. JS execution IS PROHIBITED within the native `Runtime` (MLProcessor/Renderer).  
Evidence: @ARCH-REQ::DYN-WEB-JS.

Observed signals:
- Total JS disablement rendered modern SPAs (Single Page Apps) non-functional.
- Isolating JS to a child process preserves the native performance of the core pipeline.

Relationship mapping:
- Separation of "Content Logic" (JS) from "Filtering Logic" (Rust) aligns with PIPE-MONOLITH simplicity.

## Decision: Hard-Synchronous Stop-and-Wait - DEC-HARD-SYNC-PIPE
Statement: The pipeline SHALL operate as a synchronous stop-and-wait system. The `Loader` MUST NOT send a new frame until the `Renderer` signals completion via an explicit ACK (0x01).  
Evidence: @EXP-SPIKE-03-VISUAL-WGPU.

Observed signals:
- Async queuing resulted in 100 ms+ latency drift (out-of-sync video).
- Hard-sync ACK stabilized latency to 1-2 frame intervals.
- Matches `PresentMode::Fifo` (V-Sync) hardware behavior.

Relationship mapping:
- Tight coupling of acquisition to presentation aligns with the End-to-End Latency priority.

## Notes / Explanatory
- [EXPLANATORY] This log is append-only to preserve the historical rationale.
- [EXPLANATORY] All decisions MUST reference evidence from the `EXP` class or technical spikes.
