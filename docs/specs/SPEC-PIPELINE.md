# Pipeline Specification
ID: SPEC-PIPELINE  
Status: STABLE  
Depends on: STD-DOC, SPEC-FETCHER, SPEC-ML-PROCESSOR, SPEC-RENDERER

## Pipeline Topology
The system supports two structural configurations for processing `UnitOfWork` items.

### Configuration A [Staged Pipeline]
Structure:
- `FetchStage` -> Bounded Queue -> `ProcessStage` -> Bounded Queue -> `RenderStage`.

Rules:
- Stages MUST communicate via asynchronous bounded channels.
- The pipeline MUST block ingestion IF the first queue is full.
- Stages MUST NOT drop data under backpressure.
- Ordering of `ContentBuffer` items MUST be preserved from ingestion to completion.

### Configuration B [Single-Stage]
Structure:
- `FetchStage` -> `ProcessStage` -> `RenderStage` executed as a synchronous call chain.

Rules:
- Configuration B MUST NOT use queues.
- Configuration B MUST NOT use intermediate buffering between stages.

## Data Flow Constraints
Constraint:
- Data MUST flow unidirectional from left to right.
- Reverse flow OR feedback loops are PROHIBITED.
- Stage logic MUST be identical between Configuration A AND Configuration B.

Rationale:
- Structural identity ensures that observed performance differences result from the communication model ONLY.

## Liveness and Backpressure Semantics

### Timeouts
Constraint:
- `INGESTION_TIMEOUT_MS`: 5000 ms.
- IF a `UnitOfWork` cannot be enqueued within `INGESTION_TIMEOUT_MS`, THEN the driver MUST log a `BACKPRESSURE_TIMEOUT` and terminate the ingestion attempt.
- Stage-to-stage transfers MUST NOT timeout, but MUST block if the consumer is slower than the producer.

### Shutdown Protocol
Constraint:
- The pipeline MUST use `PipelineMessage::SIGNAL(PipelineSignal::SHUTDOWN)` to manage the "Drain" operation.
- Upon receiving a shutdown signal from the `System Driver`, the `Fetcher` MUST emit a `SHUTDOWN` signal into the first queue.
- EACH subsequent stage MUST:
  1. Receive the `SHUTDOWN` signal.
  2. Complete any internal cleanup required.
  3. Propagate the `SHUTDOWN` signal to the next queue immediately.
- The `Renderer` MUST terminate its execution loop upon receiving the `SHUTDOWN` signal.

Rationale:
- Type-safe signaling eliminates the overhead of `HashMap` lookups for control flow AND prevents "Magic String" dependencies in the execution logic.

## Notes / Explanatory
- [EXPLANATORY] This specification defines the structural "Wiring" of the components.
- [EXPLANATORY] Execution semantics for individual stages are defined in their respective `SPEC` documents.
