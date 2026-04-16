# Execution Base Contract
ID: CONTR-EXEC-BASE  
Status: PRELIMINARY  
Depends on: STD-DOC, INT-DATA-MOD

## Purpose
Define strict execution semantics for benchmarking pipeline stages.

This contract defines execution boundaries only.  
It does NOT define architecture choices.

## Core Execution Unit
UnitOfWork:
- single input item processed from ingestion to final output write

InputType:
- `ContentBuffer`

OutputType:
- `ContentBuffer`

## Stage Definitions (mandatory identical semantics across configs)

### FetchStage
Function:

```
FetchStage(input: ContentBuffer) -> ContentBuffer
```

Rules:
- MUST perform input retrieval or pass-through only
- MUST NOT perform ML processing
- MUST NOT perform rendering logic
- MUST NOT modify ML-related metadata
- MAY simulate fetch delay if required by dataset rules
- MUST return a valid ContentBuffer

Failure:
- on error, MUST return status FAIL with empty payload

### ProcessStage (MLProcessor)
Function:

```
ProcessStage(input: ContentBuffer) -> ContentBuffer
```

Rules:
- MUST perform transformation on payload only
- MUST NOT perform I/O operations
- MUST NOT perform fetch logic
- MUST NOT perform rendering logic
- MUST be stateless across calls
- MUST produce deterministic output for identical input

### RenderStage
Function:

```
RenderStage(input: ContentBuffer) -> ContentBuffer
```

Rules:
- MUST serialize output to final form
- MUST NOT modify payload semantics
- MUST NOT perform ML computation
- MUST NOT influence upstream stages
- MAY simulate output write delay
- MUST return final ContentBuffer unchanged except for output metadata

## Timing Rules (critical)

### StartTime
MUST be recorded:
- at entry to FetchStage in Config A
- at function entry in Config B

### EndTime
MUST be recorded:
- immediately after RenderStage completion in Config A
- immediately after function return in Config B

No other timing points are valid.

## Execution Model Constraints

### Config A (Staged Pipeline)
Rules:
- MUST execute FetchStage → ProcessStage → RenderStage sequentially
- MUST use queue between stages
- Queue MUST preserve ordering
- Each stage MAY run in separate thread

Queue Behavior:
- bounded buffer REQUIRED
- blocking on full queue REQUIRED (no dropping allowed)

### Config B (Single Execution Flow)
Rules:
- MUST execute FetchStage, ProcessStage, RenderStage in a single call chain
- MUST NOT use queues
- MUST NOT use inter-stage buffering
- MUST behave as inlined execution of stages

## Identity Constraint (critical for comparability)
The following MUST remain identical between Config A and B:
- FetchStage logic
- ProcessStage logic
- RenderStage logic
- Input dataset
- Output serialization format

Only execution structure differs.

## Error Handling
All stages:
- MUST return explicit status SUCCESS or FAIL
- MUST NOT throw unhandled exceptions
- FAIL results MUST still be logged

## Logging Contract
Each unit of work MUST produce:

```
config_id
input_id
iteration
start_time_ms
end_time_ms
latency_ms
status
```

Rules:
- timestamps MUST use same clock source
- logging MUST occur AFTER EndTime capture
- logs MUST be append-only
- logs MUST NOT be aggregated during execution

## Non-Variability Requirement
To ensure comparability:
- same CPU allocation policy
- same thread limits
- same memory limits
- same input dataset ordering
- same runtime environment version

## Constraint Summary
This contract enforces:
- identical functional behavior across configs
- different execution structure only
- fixed measurement boundaries
- strict stage isolation rules
